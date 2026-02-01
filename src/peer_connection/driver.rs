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
        Ok(Self {
            inner,
            socket,
            sender,
            timer,
            recv_buf: vec![0u8; 2000], // Standard MTU size
            local_addr,
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
                while let Some(_message) = core.poll_read() {
                    // TODO: Handle application messages
                    // These will be handled by Track and DataChannel wrappers later
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
