//! Peer connection driver (event loop)

use super::connection::PeerConnectionInner;
use super::*;
use crate::runtime::{AsyncTimer, AsyncUdpSocket, RecvMeta, Transmit, UdpSender};
use bytes::BytesMut;
use rtc::sansio::Protocol;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use std::future::Future;
use std::io::IoSliceMut;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

/// The driver future for a peer connection
///
/// This future drives the peer connection's event loop, handling:
/// - Network I/O (receiving and sending packets)
/// - Timer management
/// - Event dispatching to the handler
///
/// Similar to Quinn's ConnectionDriver.
pub struct PeerConnectionDriver {
    inner: Arc<PeerConnectionInner>,
    socket: Box<dyn AsyncUdpSocket>,
    sender: Pin<Box<dyn UdpSender>>,
    timer: Pin<Box<dyn AsyncTimer>>,
    recv_buf: Vec<u8>,
    local_addr: SocketAddr,
    /// Channel for receiving outgoing data channel messages
    data_rx: Option<tokio::sync::mpsc::UnboundedReceiver<crate::data_channel::OutgoingMessage>>,
    /// Channel for receiving outgoing RTP packets
    rtp_rx: Option<tokio::sync::mpsc::UnboundedReceiver<crate::track::OutgoingRtpPacket>>,
    /// Channel for receiving outgoing RTCP packets from senders
    rtcp_rx: Option<tokio::sync::mpsc::UnboundedReceiver<crate::track::OutgoingRtcpPackets>>,
    /// Channel for receiving outgoing RTCP packets from receivers
    receiver_rtcp_rx:
        Option<tokio::sync::mpsc::UnboundedReceiver<crate::track::OutgoingReceiverRtcpPackets>>,
}

impl PeerConnectionDriver {
    /// Create a new driver for the given peer connection
    pub(crate) fn new(
        inner: Arc<PeerConnectionInner>,
        socket: Box<dyn AsyncUdpSocket>,
    ) -> Result<Self, std::io::Error> {
        let local_addr = socket.local_addr()?;
        let sender = socket.create_sender();
        let timer = inner.runtime.new_timer(Instant::now());

        // Take the data channel receiver (can only be done once)
        let data_rx = inner.data_rx.lock().unwrap().take();

        // Take the RTP receiver (can only be done once)
        let rtp_rx = inner.rtp_rx.lock().unwrap().take();

        // Take the RTCP receivers (can only be done once)
        let rtcp_rx = inner.rtcp_rx.lock().unwrap().take();
        let receiver_rtcp_rx = inner.receiver_rtcp_rx.lock().unwrap().take();

        Ok(Self {
            inner,
            socket,
            sender,
            timer,
            recv_buf: vec![0u8; 2000], // Standard MTU size
            local_addr,
            data_rx,
            rtp_rx,
            rtcp_rx,
            receiver_rtcp_rx,
        })
    }
}

impl Future for PeerConnectionDriver {
    type Output = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        loop {
            // 1. Send outgoing packets
            {
                let mut core = this.inner.core.lock().unwrap();
                while let Some(msg) = core.poll_write() {
                    // Send packet via UDP socket
                    let transmit = Transmit {
                        destination: msg.transport.peer_addr,
                        contents: &msg.message,
                        src_addr: Some(msg.transport.local_addr),
                    };

                    match this.sender.as_mut().poll_send(&transmit, cx) {
                        Poll::Ready(Ok(())) => {
                            log::trace!(
                                "Sent {} bytes to {:?}",
                                msg.message.len(),
                                msg.transport.peer_addr
                            );
                        }
                        Poll::Ready(Err(e)) => {
                            log::error!("Failed to send packet: {}", e);
                            return Poll::Ready(Err(e.into()));
                        }
                        Poll::Pending => {
                            // Socket not ready, will try again next poll
                            // Note: We're dropping the message here, which may cause issues
                            // TODO: Buffer unsent messages
                            break;
                        }
                    }
                }
            }

            // 1.5. Handle outgoing DataChannel messages
            if let Some(ref mut data_rx) = this.data_rx {
                while let Poll::Ready(Some(outgoing)) = data_rx.poll_recv(cx) {
                    // Create RTCMessage from OutgoingMessage
                    let rtc_message = rtc::peer_connection::message::RTCMessage::DataChannelMessage(
                        outgoing.channel_id,
                        outgoing.message,
                    );

                    // Send via core
                    let mut core = this.inner.core.lock().unwrap();
                    if let Err(e) = core.handle_write(rtc_message) {
                        log::error!("Failed to send DataChannel message: {}", e);
                        return Poll::Ready(Err(Box::new(e)));
                    }
                }
            }

            // 1.6. Handle outgoing RTP packets
            if let Some(ref mut rtp_rx) = this.rtp_rx {
                while let Poll::Ready(Some(outgoing)) = rtp_rx.poll_recv(cx) {
                    // Get sender and write RTP
                    let mut core = this.inner.core.lock().unwrap();
                    if let Some(mut sender) = core.rtp_sender(outgoing.sender_id)
                        && let Err(e) = sender.write_rtp(outgoing.packet)
                    {
                        log::error!("Failed to send RTP packet: {}", e);
                    }
                }
            }

            // 1.7. Handle outgoing RTCP packets from senders
            if let Some(ref mut rtcp_rx) = this.rtcp_rx {
                while let Poll::Ready(Some(outgoing)) = rtcp_rx.poll_recv(cx) {
                    // Get sender and write RTCP
                    let mut core = this.inner.core.lock().unwrap();
                    if let Some(mut sender) = core.rtp_sender(outgoing.sender_id) {
                        // Convert Send packets to non-Send (safe because we're about to consume them)
                        let packets: Vec<Box<dyn rtc::rtcp::Packet>> = outgoing
                            .packets
                            .into_iter()
                            .map(|p| {
                                // Re-box without Send bound
                                unsafe { std::mem::transmute(p) }
                            })
                            .collect();
                        if let Err(e) = sender.write_rtcp(packets) {
                            log::error!("Failed to send RTCP packets: {}", e);
                        }
                    }
                }
            }

            // 1.8. Handle outgoing RTCP packets from receivers
            if let Some(ref mut receiver_rtcp_rx) = this.receiver_rtcp_rx {
                while let Poll::Ready(Some(outgoing)) = receiver_rtcp_rx.poll_recv(cx) {
                    // Get receiver and write RTCP
                    let mut core = this.inner.core.lock().unwrap();
                    if let Some(mut receiver) = core.rtp_receiver(outgoing.receiver_id) {
                        // Convert Send packets to non-Send (safe because we're about to consume them)
                        let packets: Vec<Box<dyn rtc::rtcp::Packet>> = outgoing
                            .packets
                            .into_iter()
                            .map(|p| {
                                // Re-box without Send bound
                                unsafe { std::mem::transmute(p) }
                            })
                            .collect();
                        if let Err(e) = receiver.write_rtcp(packets) {
                            log::error!("Failed to send RTCP feedback: {}", e);
                        }
                    }
                }
            }

            // 2. Process events and dispatch to handler
            {
                let mut core = this.inner.core.lock().unwrap();
                while let Some(event) = core.poll_event() {
                    // Spawn event handler in background
                    let handler = this.inner.handler.clone();
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
                    this.inner.runtime.spawn(Box::pin(future));
                }
            }

            // 3. Read application messages (RTP/RTCP/data)
            {
                let mut core = this.inner.core.lock().unwrap();
                while let Some(message) = core.poll_read() {
                    match message {
                        rtc::peer_connection::message::RTCMessage::DataChannelMessage(
                            channel_id,
                            dc_message,
                        ) => {
                            // Send message to the appropriate data channel
                            let data_channel_rxs = this.inner.data_channel_rxs.lock().unwrap();
                            if let Some(tx) = data_channel_rxs.get(&channel_id) {
                                if let Err(e) = tx.send(dc_message) {
                                    log::warn!(
                                        "Failed to send message to data channel {}: {}",
                                        channel_id,
                                        e
                                    );
                                }
                            } else {
                                log::warn!(
                                    "Received message for unknown data channel: {}",
                                    channel_id
                                );
                            }
                        }
                        rtc::peer_connection::message::RTCMessage::RtpPacket(track_id, _packet) => {
                            // TODO: Handle RTP packets (will implement with Track wrappers)
                            log::trace!("Received RTP packet for track: {:?}", track_id);
                        }
                        rtc::peer_connection::message::RTCMessage::RtcpPacket(
                            track_id,
                            _packets,
                        ) => {
                            // TODO: Handle RTCP packets (will implement with Track wrappers)
                            log::trace!("Received RTCP packets for track: {:?}", track_id);
                        }
                    }
                }
            }

            // 4. Check for timer expiration
            {
                if this.timer.as_mut().poll(cx).is_ready() {
                    let mut core = this.inner.core.lock().unwrap();
                    let now = this.inner.runtime.now();
                    core.handle_timeout(now)?;

                    // Reset timer for next timeout
                    if let Some(deadline) = core.poll_timeout() {
                        this.timer.as_mut().reset(deadline);
                    }
                    continue; // Process any new events
                }
            }

            // 5. Try to receive network packets
            {
                let mut bufs = [IoSliceMut::new(&mut this.recv_buf)];
                let mut meta = [RecvMeta {
                    addr: SocketAddr::from(([0, 0, 0, 0], 0)),
                    len: 0,
                    dst_addr: None,
                }];

                match this.socket.poll_recv(cx, &mut bufs, &mut meta) {
                    Poll::Ready(Ok(n)) if n > 0 => {
                        // Feed received packet to core
                        for recv_meta in meta.iter().take(n) {
                            let packet_data = &this.recv_buf[..recv_meta.len];

                            let tagged_msg = TaggedBytesMut {
                                now: this.inner.runtime.now(),
                                transport: TransportContext {
                                    local_addr: recv_meta.dst_addr.unwrap_or(this.local_addr),
                                    peer_addr: recv_meta.addr,
                                    ecn: None,
                                    transport_protocol: TransportProtocol::UDP,
                                },
                                message: BytesMut::from(packet_data),
                            };

                            let mut core = this.inner.core.lock().unwrap();
                            if let Err(e) = core.handle_read(tagged_msg) {
                                log::error!("Error handling received packet: {}", e);
                                return Poll::Ready(Err(Box::new(e)));
                            }
                        }
                        log::trace!("Received {} packets", n);
                        continue;
                    }
                    Poll::Ready(Ok(_)) => {
                        // No packets received, continue
                    }
                    Poll::Ready(Err(e)) => {
                        log::error!("Socket receive error: {}", e);
                        return Poll::Ready(Err(e.into()));
                    }
                    Poll::Pending => {}
                }
            }

            // If we get here, nothing is ready - return pending
            return Poll::Pending;
        }
    }
}
