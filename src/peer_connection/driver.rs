//! Peer connection driver (event loop)
//!
//! Follows the rtc EventLoop pattern with async select

#![allow(clippy::collapsible_if)]

use super::connection::PeerConnectionInner;
use crate::runtime::{AsyncUdpSocket, sync};
use bytes::BytesMut;
use futures::FutureExt; // For .fuse() in futures::select!
use rtc::peer_connection::event::RTCPeerConnectionEvent;
use rtc::sansio::Protocol;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(30);

/// The driver for a peer connection
///
/// Runs the event loop following rtc's EventLoop pattern with tokio::select!
pub struct PeerConnectionDriver {
    inner: Arc<PeerConnectionInner>,
    socket: Box<dyn AsyncUdpSocket>,
    local_addr: SocketAddr,
    /// Channel for receiving outgoing data channel messages
    data_rx: Option<sync::Receiver<crate::data_channel::OutgoingMessage>>,
    /// Channel for receiving outgoing RTP packets
    rtp_rx: Option<sync::Receiver<crate::track::OutgoingRtpPacket>>,
    /// Channel for receiving outgoing RTCP packets from senders
    rtcp_rx: Option<sync::Receiver<crate::track::OutgoingRtcpPackets>>,
    /// Channel for receiving outgoing RTCP packets from receivers
    receiver_rtcp_rx: Option<sync::Receiver<crate::track::OutgoingReceiverRtcpPackets>>,
}

impl PeerConnectionDriver {
    /// Create a new driver for the given peer connection
    pub(crate) fn new(
        inner: Arc<PeerConnectionInner>,
        socket: Box<dyn AsyncUdpSocket>,
    ) -> Result<Self, std::io::Error> {
        let local_addr = socket.local_addr()?;

        // Store local address in inner for ICE gathering
        *inner.local_addr.lock().unwrap() = Some(local_addr);

        // Take the receivers (can only be done once)
        let data_rx = inner.data_rx.lock().unwrap().take();
        let rtp_rx = inner.rtp_rx.lock().unwrap().take();
        let rtcp_rx = inner.rtcp_rx.lock().unwrap().take();
        let receiver_rtcp_rx = inner.receiver_rtcp_rx.lock().unwrap().take();

        Ok(Self {
            inner,
            socket,
            local_addr,
            data_rx,
            rtp_rx,
            rtcp_rx,
            receiver_rtcp_rx,
        })
    }

    /// Run the driver event loop
    ///
    /// This follows rtc's EventLoop pattern exactly with tokio::select!
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut recv_buf = vec![0u8; 2000];

        'EventLoop: loop {
            // 1. poll_write() - Send all outgoing packets
            {
                let mut core = self.inner.core.lock().await;
                while let Some(msg) = core.poll_write() {
                    drop(core);

                    match self
                        .socket
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

                    let handler = self.inner.handler.clone();
                    let runtime = self.inner.runtime.clone();
                    let future = async move {
                        match event {
                            RTCPeerConnectionEvent::OnNegotiationNeededEvent => {
                                handler.on_negotiation_needed().await;
                            }
                            RTCPeerConnectionEvent::OnIceCandidateEvent(evt) => {
                                handler.on_ice_candidate(evt).await;
                            }
                            RTCPeerConnectionEvent::OnIceCandidateErrorEvent(evt) => {
                                handler.on_ice_candidate_error(evt).await;
                            }
                            RTCPeerConnectionEvent::OnSignalingStateChangeEvent(state) => {
                                handler.on_signaling_state_change(state).await;
                            }
                            RTCPeerConnectionEvent::OnIceConnectionStateChangeEvent(state) => {
                                handler.on_ice_connection_state_change(state).await;
                            }
                            RTCPeerConnectionEvent::OnIceGatheringStateChangeEvent(state) => {
                                handler.on_ice_gathering_state_change(state).await;
                            }
                            RTCPeerConnectionEvent::OnConnectionStateChangeEvent(state) => {
                                handler.on_connection_state_change(state).await;
                            }
                            RTCPeerConnectionEvent::OnDataChannel(evt) => {
                                handler.on_data_channel(evt).await;
                            }
                            RTCPeerConnectionEvent::OnTrack(evt) => {
                                handler.on_track(evt).await;
                            }
                        }
                    };
                    runtime.spawn(Box::pin(future));

                    core = self.inner.core.lock().await;
                }
            }

            // 3. poll_read() - Process incoming messages
            {
                let mut core = self.inner.core.lock().await;
                while let Some(message) = core.poll_read() {
                    match message {
                        rtc::peer_connection::message::RTCMessage::DataChannelMessage(
                            channel_id,
                            dc_message,
                        ) => {
                            let data_channel_rxs = self.inner.data_channel_rxs.lock().unwrap();
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
                        rtc::peer_connection::message::RTCMessage::RtpPacket(track_id, _packet) => {
                            log::trace!("Received RTP packet for track: {:?}", track_id);
                        }
                        rtc::peer_connection::message::RTCMessage::RtcpPacket(
                            track_id,
                            _packets,
                        ) => {
                            log::trace!("Received RTCP packets for track: {:?}", track_id);
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
                // Driver wake notification (from STUN gathering, etc.)
                _ = self.inner.driver_notify.notified().fuse() => {
                    log::trace!("Driver notified by background task");
                    continue 'EventLoop;
                }

                // Timer expired
                _ = timer.fuse() => {
                    {
                        let mut core = self.inner.core.lock().await;
                        core.handle_timeout(Instant::now())?;
                    }
                }

                // Outgoing data channel message
                outgoing = async {
                    match &mut self.data_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                }.fuse() => {
                    if let Some(outgoing) = outgoing {
                        let rtc_message = rtc::peer_connection::message::RTCMessage::DataChannelMessage(
                            outgoing.channel_id,
                            outgoing.message,
                        );
                        {
                            let mut core = self.inner.core.lock().await;
                            core.handle_write(rtc_message)?;
                        }
                    }
                }

                // Outgoing RTP packet
                outgoing = async {
                    match &mut self.rtp_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                }.fuse() => {
                    if let Some(outgoing) = outgoing {
                        {
                            let mut core = self.inner.core.lock().await;
                            if let Some(mut sender) = core.rtp_sender(outgoing.sender_id) {
                                if let Err(e) = sender.write_rtp(outgoing.packet) {
                                    log::error!("Failed to send RTP: {}", e);
                                }
                            }
                        }
                    }
                }

                // Outgoing RTCP from senders
                outgoing = async {
                    match &mut self.rtcp_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                }.fuse() => {
                    if let Some(outgoing) = outgoing {
                        {
                            let mut core = self.inner.core.lock().await;
                            if let Some(mut sender) = core.rtp_sender(outgoing.sender_id) {
                                let packets: Vec<Box<dyn rtc::rtcp::Packet>> = outgoing
                                    .packets
                                    .into_iter()
                                    .map(|p| unsafe { std::mem::transmute(p) })
                                    .collect();
                                if let Err(e) = sender.write_rtcp(packets) {
                                    log::error!("Failed to send RTCP: {}", e);
                                }
                            }
                        }
                    }
                }

                // Outgoing RTCP from receivers
                outgoing = async {
                    match &mut self.receiver_rtcp_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                }.fuse() => {
                    if let Some(outgoing) = outgoing {
                        {
                            let mut core = self.inner.core.lock().await;
                            if let Some(mut receiver) = core.rtp_receiver(outgoing.receiver_id) {
                                let packets: Vec<Box<dyn rtc::rtcp::Packet>> = outgoing
                                    .packets
                                    .into_iter()
                                    .map(|p| unsafe { std::mem::transmute(p) })
                                    .collect();
                                if let Err(e) = receiver.write_rtcp(packets) {
                                    log::error!("Failed to send RTCP feedback: {}", e);
                                }
                            }
                        }
                    }
                }

                // Incoming network packet
                res = self.socket.recv_from(&mut recv_buf).fuse() => {
                    match res {
                        Ok((n, peer_addr)) => {
                            log::trace!("Received {} bytes from {}", n, peer_addr);
                            let tagged_msg = TaggedBytesMut {
                                now: Instant::now(),
                                transport: TransportContext {
                                    local_addr: self.local_addr,
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
                        Err(e) => {
                            log::error!("Socket recv error: {}", e);
                            return Err(Box::new(e));
                        }
                    }
                }
            }
        }
    }
}
