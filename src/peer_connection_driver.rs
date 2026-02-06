//! Peer connection driver (event loop)
//!
//! Follows the rtc EventLoop pattern with async select

#![allow(clippy::collapsible_if)]

use crate::Result;
use crate::peer_connection::MessageInner;
use crate::peer_connection::PeerConnectionRef;
use crate::runtime::{AsyncUdpSocket, Receiver};
use bytes::BytesMut;
use futures::FutureExt; // For .fuse() in futures::select!
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::peer_connection::event::RTCPeerConnectionEvent;
use rtc::peer_connection::message::RTCMessage;
use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::sansio::Protocol;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(86400); // 1 day duration

/// The driver for a peer connection
///
/// Runs the event loop following rtc's EventLoop pattern with tokio::select!
pub struct PeerConnectionDriver<I = NoopInterceptor>
where
    I: Interceptor,
{
    inner: Arc<PeerConnectionRef<I>>,
    sockets: Vec<Arc<dyn AsyncUdpSocket>>,
    /// Channel for receiving outgoing messages
    msg_rx: Option<Receiver<MessageInner>>,
}

impl<I> PeerConnectionDriver<I>
where
    I: Interceptor,
{
    /// Create a new driver for the given peer connection
    pub(crate) async fn new(
        inner: Arc<PeerConnectionRef<I>>,
        sockets: Vec<Arc<dyn AsyncUdpSocket>>,
    ) -> Result<Self> {
        // Take the receiver (can only be done once)
        let msg_rx = inner.msg_rx.lock().await.take();

        Ok(Self {
            inner,
            sockets,
            msg_rx,
        })
    }

    /// Run the driver event loop
    ///
    /// This follows rtc's EventLoop pattern exactly with tokio::select!
    pub async fn run(&mut self) -> Result<()> {
        let mut recv_buf = vec![0u8; 2000];

        loop {
            // 1. poll_write() - Send all outgoing packets
            {
                let mut core = self.inner.core.lock().await;
                while let Some(msg) = core.poll_write() {
                    drop(core);

                    match self.sockets[0] //TODO: select correct socket
                        .send_to(&msg.message, msg.transport.peer_addr)
                        .await
                    {
                        Ok(n) => {
                            log::trace!("Sent {} bytes to {:?}", n, msg.transport.peer_addr);
                        }
                        Err(e) => {
                            log::error!("Failed to send to {:?}: {}", msg.transport.peer_addr, e);
                        }
                    }

                    core = self.inner.core.lock().await;
                }
            }

            // 2. poll_event() - Process all events
            {
                let mut core = self.inner.core.lock().await;
                while let Some(event) = core.poll_event() {
                    drop(core);

                    match event {
                        RTCPeerConnectionEvent::OnNegotiationNeededEvent => {
                            self.inner.handler.on_negotiation_needed().await;
                        }
                        RTCPeerConnectionEvent::OnIceCandidateEvent(evt) => {
                            self.inner.handler.on_ice_candidate(evt).await;
                        }
                        RTCPeerConnectionEvent::OnIceCandidateErrorEvent(evt) => {
                            self.inner.handler.on_ice_candidate_error(evt).await;
                        }
                        RTCPeerConnectionEvent::OnSignalingStateChangeEvent(state) => {
                            self.inner.handler.on_signaling_state_change(state).await;
                        }
                        RTCPeerConnectionEvent::OnIceConnectionStateChangeEvent(state) => {
                            self.inner
                                .handler
                                .on_ice_connection_state_change(state)
                                .await;
                        }
                        RTCPeerConnectionEvent::OnIceGatheringStateChangeEvent(state) => {
                            self.inner
                                .handler
                                .on_ice_gathering_state_change(state)
                                .await;
                        }
                        RTCPeerConnectionEvent::OnConnectionStateChangeEvent(state) => {
                            self.inner.handler.on_connection_state_change(state).await;
                        }
                        RTCPeerConnectionEvent::OnDataChannel(evt) => {
                            self.inner.handler.on_data_channel(evt).await;
                        }
                        RTCPeerConnectionEvent::OnTrack(evt) => {
                            self.inner.handler.on_track(evt).await;
                        }
                    }

                    core = self.inner.core.lock().await;
                }
            }

            // 3. poll_read() - Process incoming messages
            {
                let mut core = self.inner.core.lock().await;
                while let Some(message) = core.poll_read() {
                    match message {
                        RTCMessage::DataChannelMessage(channel_id, dc_message) => {
                            let data_channel_rxs = self.inner.data_channel_rxs.lock().await;
                            if let Some(tx) = data_channel_rxs.get(&channel_id) {
                                if let Err(e) = tx.try_send(dc_message) {
                                    log::warn!(
                                        "Failed to send to data channel {}: {:?}",
                                        channel_id,
                                        e
                                    );
                                }
                            }
                        }
                        RTCMessage::RtpPacket(track_id, _packet) => {
                            log::trace!("Received RTP packet for track: {:?}", track_id);
                            //TODO:
                        }
                        RTCMessage::RtcpPacket(track_id, _packets) => {
                            log::trace!("Received RTCP packets for track: {:?}", track_id);
                            //TODO:
                        }
                    }
                }
            }

            // Get next timeout
            let timeout = {
                let mut core = self.inner.core.lock().await;
                core.poll_timeout()
                    .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION)
            };

            let delay_from_now = timeout
                .checked_duration_since(Instant::now())
                .unwrap_or(Duration::from_secs(0));

            // Handle immediate timeout
            if delay_from_now.is_zero() {
                let mut core = self.inner.core.lock().await;
                core.handle_timeout(Instant::now())?;
                continue;
            }

            let timer = crate::runtime::sleep(delay_from_now);
            futures::pin_mut!(timer);

            // Runtime-agnostic select! using futures::select! (works with both tokio and smol)
            futures::select! {
                // Timer expired
                _ = timer.fuse() => {
                    {
                        let mut core = self.inner.core.lock().await;
                        core.handle_timeout(Instant::now())?;
                    }
                }

                // Inner message (DataChannel, RTP, RTCP, or ICE candidate)
                msg = async {
                    match &mut self.msg_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                }.fuse() => {
                    if let Some(msg) = msg {
                        match msg {
                            MessageInner::DataChannelMessage(channel_id, message) => {
                                let mut core = self.inner.core.lock().await;
                                if core.data_channel(channel_id).is_some() {
                                    if let Err(err) = core.handle_write(RTCMessage::DataChannelMessage(
                                        channel_id,
                                        message,
                                    )) {
                                        log::error!("Failed to send data channel message: {}", err);
                                    }
                                }
                            }
                            MessageInner::SenderRtp(sender_id, packet) => {
                                let mut core = self.inner.core.lock().await;
                                if let Some(mut sender) = core.rtp_sender(sender_id) {
                                    if let Err(err) = sender.write_rtp(packet) {
                                        log::error!("Failed to send RTP: {}", err);
                                    }
                                }
                            }
                            MessageInner::SenderRtcp(sender_id, rtcp_packets) => {
                                let mut core = self.inner.core.lock().await;
                                if let Some(mut sender) = core.rtp_sender(sender_id) {
                                    if let Err(err) = sender.write_rtcp(rtcp_packets) {
                                        log::error!("Failed to send RTCP: {}", err);
                                    }
                                }
                            }
                            MessageInner::ReceiverRtcp(receiver_id, rtcp_packets) => {
                                let mut core = self.inner.core.lock().await;
                                if let Some(mut receiver) = core.rtp_receiver(receiver_id) {
                                    if let Err(err) = receiver.write_rtcp(rtcp_packets) {
                                        log::error!("Failed to send RTCP feedback: {}", err);
                                    }
                                }
                            }
                            MessageInner::LocalIceCandidate(candidate) => {
                                let mut core = self.inner.core.lock().await;
                                if let Err(err) = core.add_local_candidate(candidate) {
                                    log::error!("Failed to add local candidate: {}", err);
                                }
                            }
                            MessageInner::IceGatheringComplete => {
                                let end_of_candidates = RTCIceCandidateInit::default();
                                let mut core = self.inner.core.lock().await;
                                if let Err(err) = core.add_local_candidate(end_of_candidates) {
                                    log::error!("Failed to add end_of_candidates: {}", err);
                                }
                            }
                            MessageInner::Close => {
                                return Ok(())
                            }
                        }
                    }
                }

                // Incoming network packet
                //TODO: select correct socket
                res = self.sockets[0].recv_from(&mut recv_buf).fuse() => {
                    match res {
                        Ok((n, peer_addr)) => {
                            log::trace!("Received {} bytes from {}", n, peer_addr);
                            let tagged_msg = TaggedBytesMut {
                                now: Instant::now(),
                                transport: TransportContext {
                                    local_addr: self.sockets[0].local_addr()?,//todo: select correct socket
                                    peer_addr,
                                    ecn: None,
                                    transport_protocol: TransportProtocol::UDP,
                                },
                                message: BytesMut::from(&recv_buf[..n]),
                            };
                            {
                                let mut core = self.inner.core.lock().await;
                                core.handle_read(tagged_msg)?;
                            }
                        }
                        Err(err) => {
                            log::error!("Socket recv error: {}", err);
                        }
                    }
                }
            }
        }
    }
}
