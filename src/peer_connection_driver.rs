//! Peer connection driver (event loop)
//!
//! Follows the rtc EventLoop pattern with async select

#![allow(clippy::collapsible_if)]

use crate::ice_gatherer::{RTCIceGatherer, RTCIceGathererEvent};
use crate::peer_connection::MessageInner;
use crate::peer_connection::PeerConnectionRef;
use crate::runtime::{AsyncUdpSocket, Receiver};
use crate::{Error, Result};
use bytes::BytesMut;
use futures::FutureExt; // For .fuse() in futures::select!
use futures::stream::{FuturesUnordered, StreamExt};
use log::{error, trace};
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::peer_connection::event::RTCPeerConnectionEvent;
use rtc::peer_connection::message::RTCMessage;
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::sansio::Protocol;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(86400); // 1 day duration

/// The driver for a peer connection
///
/// Runs the event loop following rtc's EventLoop pattern with tokio::select!
pub(crate) struct PeerConnectionDriver<I = NoopInterceptor>
where
    I: Interceptor,
{
    inner: Arc<PeerConnectionRef<I>>,
    /// ICE gatherer for managing ICE candidate gathering
    ice_gatherer: RTCIceGatherer,
    sockets: HashMap<SocketAddr, Arc<dyn AsyncUdpSocket>>,
}

impl<I> PeerConnectionDriver<I>
where
    I: Interceptor,
{
    /// Create a new driver for the given peer connection
    pub(crate) async fn new(
        inner: Arc<PeerConnectionRef<I>>,
        ice_gatherer: RTCIceGatherer,
        sockets: HashMap<SocketAddr, Arc<dyn AsyncUdpSocket>>,
    ) -> Result<Self> {
        if sockets.is_empty() {
            return Err(Error::Other("no sockets available".to_owned()));
        }

        Ok(Self {
            inner,
            ice_gatherer,
            sockets,
        })
    }

    /// Run the driver event loop
    ///
    /// This follows rtc Event Loop pattern exactly with select!
    pub(crate) async fn event_loop(&mut self, mut msg_rx: Receiver<MessageInner>) -> Result<()> {
        // Collect socket info into a vec for indexed access
        let socket_list: Vec<(SocketAddr, Arc<dyn AsyncUdpSocket>)> = self
            .sockets
            .iter()
            .map(|(addr, sock)| (*addr, sock.clone()))
            .collect();

        // Pre-allocate buffers once - one per socket, these will be reused forever
        let mut socket_buffers: Vec<Vec<u8>> =
            socket_list.iter().map(|_| vec![0u8; 2000]).collect();

        // Helper function to create a recv future for a specific socket
        let create_socket_recv_future = |idx: usize,
                                         local_addr: SocketAddr,
                                         socket: Arc<dyn AsyncUdpSocket>,
                                         mut buf: Vec<u8>| async move {
            let (n, peer_addr) = socket.recv_from(&mut buf).await?;
            Ok::<_, std::io::Error>((n, local_addr, peer_addr, idx, buf))
        };

        // Create initial set of futures in FuturesUnordered
        let mut socket_recv_futures: FuturesUnordered<_> = socket_list
            .iter()
            .enumerate()
            .map(|(idx, (local_addr, socket))| {
                let buf = std::mem::take(&mut socket_buffers[idx]);
                create_socket_recv_future(idx, *local_addr, socket.clone(), buf).boxed()
            })
            .collect();

        loop {
            // 1.a ice_gatherer poll_write()
            {
                while let Some(msg) = self.ice_gatherer.poll_write() {
                    self.handle_write(msg).await;
                }
            }

            // 1.b peer_connection poll_write() - Send all outgoing packets
            {
                let mut core = self.inner.core.lock().await;
                while let Some(msg) = core.poll_write() {
                    drop(core);
                    self.handle_write(msg).await;
                    core = self.inner.core.lock().await;
                }
            }

            // 2.a ice_gatherer poll_event()
            {
                while let Some(event) = self.ice_gatherer.poll_event() {
                    self.handle_gather_event(event).await;
                }
            }

            // 2.b peer_connection poll_event() - Process all events
            {
                let mut core = self.inner.core.lock().await;
                while let Some(event) = core.poll_event() {
                    drop(core);
                    self.handle_rtc_event(event).await;
                    core = self.inner.core.lock().await;
                }
            }

            // 3.a no need for ice_gatherer poll_read()

            // 3.b peer_connection poll_read() - Process incoming messages
            {
                let mut core = self.inner.core.lock().await;
                while let Some(message) = core.poll_read() {
                    drop(core);
                    self.handle_rtc_message(message).await;
                    core = self.inner.core.lock().await;
                }
            }

            // 4.a poll next timeout
            let mut timeout = {
                let mut core = self.inner.core.lock().await;
                core.poll_timeout()
                    .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION)
            };
            if let Some(ice_gatherer_timeout) = self.ice_gatherer.poll_timeout() {
                if ice_gatherer_timeout < timeout {
                    timeout = ice_gatherer_timeout;
                }
            }

            let delay_from_now = timeout
                .checked_duration_since(Instant::now())
                .unwrap_or(Duration::from_secs(0));

            // 4.b handle immediate timeout
            if delay_from_now.is_zero() {
                let now = Instant::now();
                self.ice_gatherer.handle_timeout(now)?;
                let mut core = self.inner.core.lock().await;
                core.handle_timeout(now)?;
                continue;
            }

            let timer = crate::runtime::sleep(delay_from_now);
            futures::pin_mut!(timer);

            // Runtime-agnostic select! using futures::select! (works with both tokio and smol)
            futures::select! {
                // Timer expired
                _ = timer.fuse() => {
                    let now = Instant::now();
                    self.ice_gatherer.handle_timeout(now)?;
                    let mut core = self.inner.core.lock().await;
                    core.handle_timeout(now)?;
                }

                // Inner message (DataChannel, RTP, RTCP, or ICE candidate)
                msg = msg_rx.recv().fuse() => {
                    if let Some(msg) = msg {
                        if self.handle_inner_message(msg).await {
                            return Ok(());
                        }
                    }
                }

                // Incoming network packet from any socket
                result = socket_recv_futures.next().fuse() => {
                    match result {
                        Some(Ok((n, local_addr, peer_addr, idx, buf))) => {
                            trace!("Received {} bytes from {} to {}", n, peer_addr, local_addr);

                            if let Err(err) = self.handle_read(TaggedBytesMut {
                                now: Instant::now(),
                                transport: TransportContext {
                                    local_addr,
                                    peer_addr,
                                    ecn: None,
                                    transport_protocol: TransportProtocol::UDP,
                                },
                                message: BytesMut::from(&buf[..n]),
                            }).await {
                                 error!("handle_read error: {}", err);
                            }

                            // Immediately create a new future for this socket and reuse the buffer
                            let (socket_local_addr, socket) = &socket_list[idx];
                            socket_recv_futures.push(
                                create_socket_recv_future(idx, *socket_local_addr, socket.clone(), buf).boxed()
                            );
                        }
                        Some(Err(err)) => {
                            error!("Socket recv error: {}", err);
                            // On error, we lost the buffer, create a new one and restart this socket
                            // This should be rare (only on actual socket errors)
                            // For now, we return the error to stop the loop
                            return Err(err.into());
                        }
                        None => {
                            // All socket futures completed (should never happen in normal operation)
                            return Err(Error::Other("all socket futures completed".to_owned()));
                        }
                    }
                }
            }
        }
    }

    async fn handle_write(&self, msg: TaggedBytesMut) {
        if let Some(socket) = self.sockets.get(&msg.transport.local_addr) {
            match socket.send_to(&msg.message, msg.transport.peer_addr).await {
                Ok(n) => {
                    trace!(
                        "Sent {} bytes to {:?} from {:?}",
                        n, msg.transport.peer_addr, msg.transport.local_addr
                    );
                }
                Err(e) => {
                    error!(
                        "Failed to send to {:?} from {:?}: {}",
                        msg.transport.peer_addr, msg.transport.local_addr, e
                    );
                }
            }
        }
    }

    async fn handle_read(&mut self, msg: TaggedBytesMut) -> Result<()> {
        if self.ice_gatherer.is_ice_message(&msg) {
            self.ice_gatherer.handle_read(msg)?;
        } else {
            let mut core = self.inner.core.lock().await;
            core.handle_read(msg)?;
        }

        Ok(())
    }

    async fn handle_gather_event(&mut self, event: RTCIceGathererEvent) {
        match event {
            RTCIceGathererEvent::LocalIceCandidate(candidate) => {
                trace!("LocalIceCandidate {:?}", candidate);
                let mut core = self.inner.core.lock().await;
                if let Err(err) = core.add_local_candidate(candidate) {
                    error!("Failed to add local candidate: {}", err);
                }
            }
            RTCIceGathererEvent::IceGatheringComplete => {
                let end_of_candidates = RTCIceCandidateInit::default();
                let mut core = self.inner.core.lock().await;
                if let Err(err) = core.add_local_candidate(end_of_candidates) {
                    error!("Failed to add end_of_candidates: {}", err);
                }
            }
        }
    }

    async fn handle_rtc_event(&mut self, event: RTCPeerConnectionEvent) {
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
            RTCPeerConnectionEvent::OnDataChannel(_evt) => {
                /*match evt {
                    RTCDataChannelEvent::OnOpen(id) => {}
                    RTCDataChannelEvent::OnError(id) => {}
                    RTCDataChannelEvent::OnClosing(id) => {}
                    RTCDataChannelEvent::OnClose(id) => {}
                    RTCDataChannelEvent::OnBufferedAmountLow(id) => {}
                    RTCDataChannelEvent::OnBufferedAmountHigh(id) => {}
                }*/
                //TODO: self.inner.handler.on_data_channel(evt).await;
            }
            RTCPeerConnectionEvent::OnTrack(_evt) => {
                //TODO: self.inner.handler.on_track(evt).await;
            }
        }
    }

    async fn handle_rtc_message(&mut self, message: RTCMessage) {
        match message {
            RTCMessage::DataChannelMessage(_channel_id, _dc_message) => {
                /*let data_channel = self.inner.data_channels.lock().await;
                if let Some(_dc) = data_channel.get(&channel_id) {
                    /*TODO:if let Err(e) = dc.on_message(dc_message).await {
                        warn!("Failed to send to data channel {}: {:?}", channel_id, e);
                    }*/
                }*/
            }
            RTCMessage::RtpPacket(track_id, _packet) => {
                trace!("Received RTP packet for track: {:?}", track_id);
                //TODO:
            }
            RTCMessage::RtcpPacket(track_id, _packets) => {
                trace!("Received RTCP packets for track: {:?}", track_id);
                //TODO:
            }
        }
    }

    async fn handle_inner_message(&mut self, msg: MessageInner) -> bool {
        match msg {
            MessageInner::DataChannelMessage(channel_id, message) => {
                let mut core = self.inner.core.lock().await;
                if core.data_channel(channel_id).is_some() {
                    if let Err(err) =
                        core.handle_write(RTCMessage::DataChannelMessage(channel_id, message))
                    {
                        error!("Failed to send data channel message: {}", err);
                    }
                }
            }
            MessageInner::SenderRtp(sender_id, packet) => {
                let mut core = self.inner.core.lock().await;
                if let Some(mut sender) = core.rtp_sender(sender_id) {
                    if let Err(err) = sender.write_rtp(packet) {
                        error!("Failed to send RTP: {}", err);
                    }
                }
            }
            MessageInner::SenderRtcp(sender_id, rtcp_packets) => {
                let mut core = self.inner.core.lock().await;
                if let Some(mut sender) = core.rtp_sender(sender_id) {
                    if let Err(err) = sender.write_rtcp(rtcp_packets) {
                        error!("Failed to send RTCP: {}", err);
                    }
                }
            }
            MessageInner::ReceiverRtcp(receiver_id, rtcp_packets) => {
                let mut core = self.inner.core.lock().await;
                if let Some(mut receiver) = core.rtp_receiver(receiver_id) {
                    if let Err(err) = receiver.write_rtcp(rtcp_packets) {
                        error!("Failed to send RTCP feedback: {}", err);
                    }
                }
            }
            MessageInner::IceGathering => {
                if self.ice_gatherer.state() != RTCIceGatheringState::Gathering {
                    if let Err(err) = self.ice_gatherer.gather().await {
                        error!("Failed to gather ice gathering: {}", err);
                    }
                }
            }
            MessageInner::Close => return true,
        }

        false
    }
}
