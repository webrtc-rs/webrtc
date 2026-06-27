//! Peer connection driver (event loop)
//!
//! Follows the rtc EventLoop pattern with async select

#![allow(clippy::collapsible_if)]

use super::transports::stun_gatherer::{
    RTCStunGatherEventIn, RTCStunGatherEventOut, RTCStunGatherer,
};
use super::transports::turn_relayer::{RTCTurnRelayEventIn, RTCTurnRelayEventOut, RTCTurnRelayer};
use crate::data_channel::{DataChannelEvent, DataChannelImpl};
use crate::media_stream::track_remote::static_rtp::TrackRemoteStaticRTP;
use crate::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use crate::peer_connection::PeerConnectionRef;
use crate::peer_connection::transports::tcp_transport::RTCTcpTransport;
use crate::peer_connection::transports::{
    SocketRecvResult, TcpReadResult, is_retryable_socket_recv_error,
};
use crate::rtp_transceiver::rtp_receiver::RtpReceiverImpl;
use crate::rtp_transceiver::{RtpReceiver, RtpTransceiverImpl};
use crate::runtime::{AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, Receiver, channel};
use bytes::BytesMut;
use futures::FutureExt; // For .fuse() in futures::select!
use futures::future::OptionFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{error, trace};
use rtc::ice::candidate::Candidate;
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::mdns::MDNS_PORT;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCIceTransportPolicy;
use rtc::peer_connection::event::{RTCDataChannelEvent, RTCPeerConnectionEvent, RTCTrackEvent};
use rtc::peer_connection::message::RTCMessage;
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::peer_connection::transport::{
    CandidateConfig, CandidateHostConfig, RTCIceCandidate, RTCIceCandidateInit,
};
use rtc::rtp_transceiver::{RTCRtpReceiverId, RTCRtpSenderId};
use rtc::sansio::Protocol;
use rtc::shared::error::{Error, Result};
use rtc::shared::tcp_framing::{TcpFrameDecoder, frame_packet};
use rtc::shared::{FourTuple, TaggedBytesMut, TransportContext, TransportProtocol};
use rtc::{rtcp, rtp};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Capacity of the internal driver event channel (WriteNotify, IceGathering, Close, …).
pub(crate) const PEER_CONNECTION_DRIVER_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Capacity of each data-channel event channel (OnOpen, OnMessage, OnClose, …).
pub(crate) const DATA_CHANNEL_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Capacity of each track-remote event channel (OnMute, OnUnmute, OnEnded, OnRtpPacket, OnRtcpPacket, …).
pub(crate) const TRACK_REMOTE_EVENT_CHANNEL_CAPACITY: usize = 256;

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(86400); // 1 day duration

/// Unified inner message type for the peer connection driver
#[derive(Debug)]
pub(crate) enum PeerConnectionDriverEvent {
    SenderRtp(RTCRtpSenderId, rtp::Packet),
    SenderRtcp(RTCRtpSenderId, Vec<Box<dyn rtcp::Packet>>),
    ReceiverRtcp(RTCRtpReceiverId, Vec<Box<dyn rtcp::Packet>>),
    RemoteIceTcpPassiveCandidate(Candidate),
    IncomingTcpStream(FourTuple, Arc<dyn AsyncTcpStream>),
    WriteNotify,
    IceGathering,
    Close,
}

/// The driver for a peer connection
///
/// Runs the event loop following rtc's EventLoop pattern with select!
pub(crate) struct PeerConnectionDriver<I = NoopInterceptor>
where
    I: Interceptor,
{
    inner: Arc<PeerConnectionRef<I>>,
    stun_gatherer: RTCStunGatherer,
    turn_relayer: RTCTurnRelayer,
    tcp_transport: RTCTcpTransport,
    mdns_socket: Option<Arc<dyn AsyncUdpSocket>>,
    udp_sockets: HashMap<SocketAddr, Arc<dyn AsyncUdpSocket>>,
    ice_gathering_active: bool,
    stun_gathering_complete: bool,
    turn_gathering_complete: bool,
}

impl<I> PeerConnectionDriver<I>
where
    I: Interceptor,
{
    /// Create a new driver for the given peer connection
    pub(crate) async fn new(
        inner: Arc<PeerConnectionRef<I>>,
        stun_gatherer: RTCStunGatherer,
        turn_relayer: RTCTurnRelayer,
        mdns_socket: Option<Arc<dyn AsyncUdpSocket>>,
        udp_sockets: HashMap<SocketAddr, Arc<dyn AsyncUdpSocket>>,
        tcp_listeners: HashMap<SocketAddr, Arc<dyn AsyncTcpListener>>,
    ) -> Result<Self> {
        if udp_sockets.is_empty() && tcp_listeners.is_empty() {
            return Err(Error::Other("no sockets or listeners available".to_owned()));
        }

        Ok(Self {
            inner,
            stun_gatherer,
            turn_relayer,
            mdns_socket,
            udp_sockets,
            tcp_transport: RTCTcpTransport::new(tcp_listeners),
            ice_gathering_active: false,
            stun_gathering_complete: false,
            turn_gathering_complete: false,
        })
    }

    /// Run the driver event loop
    ///
    /// This follows rtc Event Loop pattern exactly with select!
    pub(crate) async fn event_loop(
        &mut self,
        mut driver_event_rx: Receiver<PeerConnectionDriverEvent>,
    ) -> Result<()> {
        // Collect socket info into a vec for indexed access
        let udp_socket_list: Vec<(SocketAddr, Arc<dyn AsyncUdpSocket>)> = self
            .udp_sockets
            .iter()
            .map(|(addr, sock)| (*addr, sock.clone()))
            .chain(self.mdns_socket.iter().filter_map(|socket| {
                socket
                    .local_addr()
                    .ok()
                    .map(|local_addr| (local_addr, socket.clone()))
            }))
            .collect();

        // Pre-allocate buffers once - one per socket, these will be reused forever
        let mut udp_socket_buffers: Vec<Vec<u8>> =
            udp_socket_list.iter().map(|_| vec![0u8; 2000]).collect();

        // Helper function to create a recv future for a specific socket
        let create_udp_recv_future = |idx: usize,
                                      local_addr: SocketAddr,
                                      socket: Arc<dyn AsyncUdpSocket>,
                                      mut buf: Vec<u8>| async move {
            match socket.recv_from(&mut buf).await {
                Ok((n, peer_addr)) => SocketRecvResult::Packet {
                    n,
                    local_addr,
                    peer_addr,
                    idx,
                    buf,
                },
                Err(err) => SocketRecvResult::Error {
                    err,
                    local_addr,
                    idx,
                    buf,
                },
            }
        };

        // Create initial set of futures in FuturesUnordered
        let mut udp_recv_futures: FuturesUnordered<_> = udp_socket_list
            .iter()
            .enumerate()
            .map(|(idx, (local_addr, socket))| {
                let buf = std::mem::take(&mut udp_socket_buffers[idx]);
                create_udp_recv_future(idx, *local_addr, socket.clone(), buf).boxed()
            })
            .collect();
        let mut active_socket_count = udp_socket_list.len();

        let tcp_listeners: Vec<(SocketAddr, Arc<dyn AsyncTcpListener>)> = self
            .tcp_transport
            .listeners
            .iter()
            .map(|(addr, listener)| (*addr, listener.clone()))
            .collect();

        let mut tcp_accept_futures: FuturesUnordered<_> = tcp_listeners
            .into_iter()
            .map(|(local_addr, listener)| {
                async move {
                    match listener.accept().await {
                        Ok((stream, peer_addr)) => (local_addr, Ok((stream, peer_addr))),
                        Err(err) => (local_addr, Err(err)),
                    }
                }
                .boxed()
            })
            .collect();

        let mut tcp_read_futures = FuturesUnordered::new();

        let create_tcp_read_future = |four_tuple: FourTuple, stream: Arc<dyn AsyncTcpStream>| async move {
            let mut buf = vec![0u8; 4096];
            match stream.read(&mut buf).await {
                Ok(n) => TcpReadResult::Packet { four_tuple, n, buf },
                Err(err) => TcpReadResult::Error {
                    four_tuple,
                    err,
                    buf,
                },
            }
        };

        loop {
            // 1.a stun_gatherer poll_write()
            {
                while let Some(msg) = self.stun_gatherer.poll_write() {
                    let four_tuple: FourTuple = FourTuple::from(&msg.transport);
                    if let Err(err) = self.handle_write(msg).await {
                        error!(
                            "Failed to write packet to {:?} from {:?}: {}",
                            four_tuple.peer_addr, four_tuple.local_addr, err
                        );
                        if let Err(err) = self
                            .stun_gatherer
                            .handle_event(RTCStunGatherEventIn::SocketWriteFailure(four_tuple))
                        {
                            error!(
                                "Failed to handle event in stun_gatherer to {:?} from {:?}: {}",
                                four_tuple.peer_addr, four_tuple.local_addr, err
                            );
                        }
                    }
                }
            }

            // 1.b turn_relayer poll_write()
            {
                while let Some(msg) = self.turn_relayer.poll_write() {
                    let four_tuple: FourTuple = FourTuple::from(&msg.transport);
                    if let Err(err) = self.handle_write(msg).await {
                        error!(
                            "Failed to write packet to {:?} from {:?}: {}",
                            four_tuple.peer_addr, four_tuple.local_addr, err
                        );
                        if let Err(err) = self
                            .turn_relayer
                            .handle_event(RTCTurnRelayEventIn::SocketWriteFailure(four_tuple))
                        {
                            error!(
                                "Failed to handle event in turn_relayer to {:?} from {:?}: {}",
                                four_tuple.peer_addr, four_tuple.local_addr, err
                            );
                        }
                    }
                }
            }

            // 1.c peer_connection poll_write() - Send all outgoing packets
            {
                let mut core = self.inner.core.lock().await;
                while let Some(msg) = core.poll_write() {
                    drop(core);
                    let four_tuple: FourTuple = FourTuple::from(&msg.transport);
                    if let Err(err) = self.handle_write(msg).await {
                        error!(
                            "Failed to write packet to {:?} from {:?}: {}",
                            four_tuple.peer_addr, four_tuple.local_addr, err
                        );
                    }
                    core = self.inner.core.lock().await;
                }
            }

            // 2.a stun_gatherer poll_event()
            {
                while let Some(event) = self.stun_gatherer.poll_event() {
                    self.handle_stun_gather_event(event).await;
                }
            }

            // 2.b turn_relayer poll_event()
            {
                while let Some(event) = self.turn_relayer.poll_event() {
                    self.handle_turn_relay_event(event).await;
                }
            }

            // 2.c peer_connection poll_event() - Process all events
            {
                let mut core = self.inner.core.lock().await;
                while let Some(event) = core.poll_event() {
                    drop(core);
                    self.handle_rtc_event(event).await;
                    core = self.inner.core.lock().await;
                }
            }

            // 3.a turn_relayer poll_read() - deliver decapsulated relay data,
            // but no need for stun_gatherer poll_read()
            {
                while let Some(message) = self.turn_relayer.poll_read() {
                    let mut core = self.inner.core.lock().await;
                    core.handle_read(message)?;
                }
            }

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
            let core_timeout = {
                let mut core = self.inner.core.lock().await;
                core.poll_timeout()
            };
            let mut timeout = core_timeout.unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION);
            let stun_timeout = self.stun_gatherer.poll_timeout();
            if let Some(t) = stun_timeout {
                if t < timeout {
                    timeout = t;
                }
            }
            let turn_timeout = self.turn_relayer.poll_timeout();
            if let Some(t) = turn_timeout {
                if t < timeout {
                    timeout = t;
                }
            }

            let now = Instant::now();
            let delay_from_now = timeout
                .checked_duration_since(now)
                .unwrap_or(Duration::from_secs(0));

            trace!(
                "Timeout calculation: core={:?}, stun={:?}, turn={:?}, timeout={:?}, now={:?}, delay={:?}",
                core_timeout.map(|t| t.checked_duration_since(now)),
                stun_timeout.map(|t| t.checked_duration_since(now)),
                turn_timeout.map(|t| t.checked_duration_since(now)),
                timeout.checked_duration_since(now),
                now,
                delay_from_now
            );

            // 4.b handle immediate timeout
            if delay_from_now.is_zero() {
                self.stun_gatherer.handle_timeout(now)?;
                self.turn_relayer.handle_timeout(now)?;
                let mut core = self.inner.core.lock().await;
                core.handle_timeout(now)?;
                continue;
            }

            let timer = crate::runtime::sleep(delay_from_now);
            futures::pin_mut!(timer);

            let udp_recv_future: OptionFuture<_> = if !udp_recv_futures.is_empty() {
                Some(udp_recv_futures.next())
            } else {
                None
            }
            .into();
            futures::pin_mut!(udp_recv_future);

            let tcp_accept_future: OptionFuture<_> = if !tcp_accept_futures.is_empty() {
                Some(tcp_accept_futures.next())
            } else {
                None
            }
            .into();
            futures::pin_mut!(tcp_accept_future);

            let tcp_read_future: OptionFuture<_> = if !tcp_read_futures.is_empty() {
                Some(tcp_read_futures.next())
            } else {
                None
            }
            .into();
            futures::pin_mut!(tcp_read_future);

            // Runtime-agnostic select!
            futures::select! {
                // Timer expired
                _ = timer.fuse() => {
                    let now = Instant::now();
                    self.stun_gatherer.handle_timeout(now)?;
                    self.turn_relayer.handle_timeout(now)?;
                    let mut core = self.inner.core.lock().await;
                    core.handle_timeout(now)?;
                }

                // Driver events (RTP, RTCP, or ICE candidate)
                evt = driver_event_rx.recv().fuse() => {
                    if let Some(evt) = evt {
                        if let PeerConnectionDriverEvent::IncomingTcpStream(four_tuple, stream) = evt {
                            trace!("TCP stream connection established: {:?}", four_tuple);
                            self.tcp_transport.streams.insert(four_tuple, stream.clone());
                            self.tcp_transport.decoders.insert(four_tuple, TcpFrameDecoder::new());
                            tcp_read_futures.push(
                                create_tcp_read_future(four_tuple, stream).boxed()
                            );
                        } else {
                            let is_closed = self.handle_driver_event(evt).await;
                            if is_closed {
                                trace!("Driver event channel closed, exiting event loop");
                                return Ok(());
                            }
                        }
                    }
                }

                // Incoming network packet from any udp socket
                udp_recv_result = udp_recv_future => {
                    if let Some(res) = udp_recv_result {
                        match res {
                            Some(SocketRecvResult::Packet { n, local_addr, peer_addr, idx, buf }) => {
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
                                let (socket_local_addr, socket) = &udp_socket_list[idx];
                                udp_recv_futures.push(
                                    create_udp_recv_future(idx, *socket_local_addr, socket.clone(), buf).boxed()
                                );
                            }
                            Some(SocketRecvResult::Error { err, local_addr, idx, buf }) => {
                                if is_retryable_socket_recv_error(&err) {
                                    trace!("Transient socket recv error on {}: {}", local_addr, err);

                                    let (socket_local_addr, socket) = &udp_socket_list[idx];
                                    udp_recv_futures.push(
                                        create_udp_recv_future(idx, *socket_local_addr, socket.clone(), buf).boxed()
                                    );
                                    continue;
                                }

                                error!("Socket recv error on {}: {}", local_addr, err);
                                self.udp_sockets.remove(&local_addr);
                                active_socket_count -= 1;

                                if active_socket_count == 0 && self.tcp_transport.listeners.is_empty() {
                                    return Err(err.into());
                                }
                            }
                            None => {
                                // All socket futures completed (should never happen in normal operation)
                                if self.tcp_transport.listeners.is_empty() {
                                    return Err(Error::Other("all socket futures completed".to_owned()));
                                }
                            }
                        }
                    }
                }

                // Incoming TCP connection from any tcp listener
                tcp_accept_result = tcp_accept_future => {
                    if let Some(Some((local_addr, res))) = tcp_accept_result {
                        match res {
                            Ok((stream, peer_addr)) => {
                                let stream_local_addr = stream.local_addr().unwrap_or(local_addr);
                                let four_tuple = FourTuple {
                                    local_addr: stream_local_addr,
                                    peer_addr,
                                };
                                trace!("Accepted TCP stream on {} from {}", stream_local_addr, peer_addr);
                                self.tcp_transport.streams.insert(four_tuple, stream.clone());
                                self.tcp_transport.decoders.insert(four_tuple, TcpFrameDecoder::new());
                                tcp_read_futures.push(
                                    create_tcp_read_future(four_tuple, stream).boxed()
                                );
                            }
                            Err(err) => {
                                error!("TCP accept error: {}", err);
                            }
                        }
                        if let Some(listener) = self.tcp_transport.listeners.get(&local_addr).cloned() {
                            tcp_accept_futures.push(async move {
                                match listener.accept().await {
                                    Ok((stream, peer_addr)) => (local_addr, Ok((stream, peer_addr))),
                                    Err(err) => (local_addr, Err(err)),
                                }
                            }.boxed());
                        }
                    }
                }

                // Incoming TCP frame data from any tcp stream
                tcp_read_result = tcp_read_future => {
                    if let Some(Some(res) ) = tcp_read_result {
                        match res {
                            TcpReadResult::Packet { four_tuple, n, buf } => {
                                if n == 0 {
                                    trace!("TCP connection EOF for {:?}", four_tuple);
                                    self.tcp_transport.streams.remove(&four_tuple);
                                    self.tcp_transport.decoders.remove(&four_tuple);
                                } else {
                                    let mut packets = Vec::new();
                                    if let Some(decoder) = self.tcp_transport.decoders.get_mut(&four_tuple) {
                                        decoder.extend_from_slice(&buf[..n]);
                                        while let Some(packet) = decoder.next_packet() {
                                            packets.push(packet);
                                        }
                                    }
                                    for packet in packets {
                                        if let Err(err) = self.handle_read(TaggedBytesMut {
                                            now: Instant::now(),
                                            transport: TransportContext {
                                                local_addr: four_tuple.local_addr,
                                                peer_addr: four_tuple.peer_addr,
                                                ecn: None,
                                                transport_protocol: TransportProtocol::TCP,
                                            },
                                            message: BytesMut::from(&packet[..]),
                                        }).await {
                                            error!("handle_read error on TCP: {}", err);
                                        }
                                    }
                                    if let Some(stream) = self.tcp_transport.streams.get(&four_tuple).cloned() {
                                        tcp_read_futures.push(
                                            create_tcp_read_future(four_tuple, stream).boxed()
                                        );
                                    }
                                }
                            }
                            TcpReadResult::Error { four_tuple, err, buf: _ } => {
                                if is_retryable_socket_recv_error(&err) {
                                    trace!("Transient TCP read error on {:?}: {}", four_tuple, err);
                                    if let Some(stream) = self.tcp_transport.streams.get(&four_tuple).cloned() {
                                        tcp_read_futures.push(
                                            create_tcp_read_future(four_tuple, stream).boxed()
                                        );
                                    }
                                } else {
                                    error!("TCP read error on {:?}: {}", four_tuple, err);
                                    self.tcp_transport.streams.remove(&four_tuple);
                                    self.tcp_transport.decoders.remove(&four_tuple);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn handle_write(&mut self, msg: TaggedBytesMut) -> Result<usize> {
        let four_tuple = FourTuple::from(&msg.transport);

        let tcp_stream = if msg.transport.transport_protocol == TransportProtocol::TCP {
            // Must go over TCP
            let mut stream = self.tcp_transport.streams.get(&four_tuple).cloned();
            if stream.is_none() {
                stream = self
                    .tcp_transport
                    .streams
                    .values()
                    .find(|s| {
                        if let Ok(peer) = s.peer_addr() {
                            peer == msg.transport.peer_addr
                        } else {
                            false
                        }
                    })
                    .cloned();
            }
            if stream.is_none() {
                trace!("No TCP stream found for {:?}", four_tuple);
                return Ok(0);
            }
            stream
        } else {
            // UDP or other protocols
            if msg.transport.peer_addr.port() == MDNS_PORT {
                if let Some(socket) = &self.mdns_socket {
                    return Ok(socket
                        .send_to(&msg.message, msg.transport.peer_addr)
                        .await?);
                } else {
                    trace!(
                        "None mDNS socket, drop the packet to {:?} from {:?}",
                        msg.transport.peer_addr, msg.transport.local_addr
                    );
                    return Ok(0);
                }
            } else if self
                .turn_relayer
                .contains_local_addr(msg.transport.local_addr)
            {
                let n = msg.message.len();
                self.turn_relayer.handle_write(msg)?;
                return Ok(n);
            } else if let Some(socket) = self.udp_sockets.get(&msg.transport.local_addr) {
                return Ok(socket
                    .send_to(&msg.message, msg.transport.peer_addr)
                    .await?);
            } else {
                // If there's no UDP socket for this local address, check if we have a TCP stream
                // for the remote address (e.g. DTLS/SCTP traffic when the selected path is TCP)
                let mut stream = self.tcp_transport.streams.get(&four_tuple).cloned();
                if stream.is_none() {
                    stream = self
                        .tcp_transport
                        .streams
                        .values()
                        .find(|s| {
                            if let Ok(peer) = s.peer_addr() {
                                peer == msg.transport.peer_addr
                            } else {
                                false
                            }
                        })
                        .cloned();
                }
                if stream.is_none() {
                    trace!(
                        "None udp socket or TCP stream, drop the packet to {:?} from {:?}",
                        msg.transport.peer_addr, msg.transport.local_addr
                    );
                    return Ok(0);
                }
                stream
            }
        };

        if let Some(stream) = tcp_stream {
            let framed = frame_packet(&msg.message);
            stream.write_all(&framed).await?;
            return Ok(msg.message.len());
        }

        Ok(0)
    }

    async fn handle_read(&mut self, msg: TaggedBytesMut) -> Result<()> {
        if self.turn_relayer.is_turn_message(&msg) {
            self.turn_relayer.handle_read(msg)?;
        } else if self.stun_gatherer.is_stun_message(&msg) {
            self.stun_gatherer.handle_read(msg)?;
        } else {
            let mut core = self.inner.core.lock().await;
            core.handle_read(msg)?;
        }

        Ok(())
    }

    async fn handle_stun_gather_event(&mut self, event: RTCStunGatherEventOut) {
        match event {
            RTCStunGatherEventOut::LocalIceCandidate(candidate) => {
                trace!("LocalIceCandidate {:?}", candidate);
                let mut core = self.inner.core.lock().await;
                if let Err(err) = core.add_local_candidate(candidate) {
                    error!("Failed to add local candidate: {}", err);
                }
            }
            RTCStunGatherEventOut::StunGatheringComplete => {
                self.stun_gathering_complete = true;
                self.finish_gathering_if_ready().await;
            }
        }
    }

    async fn handle_turn_relay_event(&mut self, event: RTCTurnRelayEventOut) {
        match event {
            RTCTurnRelayEventOut::LocalIceCandidate(candidate) => {
                trace!("LocalRelayCandidate {:?}", candidate);
                let mut core = self.inner.core.lock().await;
                if let Err(err) = core.add_local_candidate(candidate) {
                    error!("Failed to add relay local candidate: {}", err);
                }
            }
            RTCTurnRelayEventOut::TurnGatheringComplete => {
                self.turn_gathering_complete = true;
                self.finish_gathering_if_ready().await;
            }
        }
    }

    async fn finish_gathering_if_ready(&mut self) {
        if self.ice_gathering_active && self.stun_gathering_complete && self.turn_gathering_complete
        {
            self.ice_gathering_active = false;
            let end_of_candidates = RTCIceCandidateInit::default();
            let mut core = self.inner.core.lock().await;
            if let Err(err) = core.add_local_candidate(end_of_candidates) {
                error!("Failed to add end_of_candidates: {}", err);
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
            RTCPeerConnectionEvent::OnDataChannel(evt) => {
                let channel_id = match evt {
                    RTCDataChannelEvent::OnOpen(id) => id,
                    RTCDataChannelEvent::OnError(id) => id,
                    RTCDataChannelEvent::OnClosing(id) => id,
                    RTCDataChannelEvent::OnClose(id) => id,
                    RTCDataChannelEvent::OnBufferedAmountLow(id) => id,
                    RTCDataChannelEvent::OnBufferedAmountHigh(id) => id,
                };

                if let RTCDataChannelEvent::OnOpen(_) = &evt {
                    let data_channel_exist = {
                        let mut core = self.inner.core.lock().await;
                        core.data_channel(channel_id).is_some()
                    };

                    if data_channel_exist {
                        let (evt_tx, evt_rx) = channel(DATA_CHANNEL_EVENT_CHANNEL_CAPACITY);
                        let data_channel =
                            Arc::new(DataChannelImpl::new(channel_id, self.inner.clone(), evt_rx));

                        {
                            let mut data_channels = self.inner.data_channel_events_tx.lock().await;
                            if let Entry::Vacant(e) = data_channels.entry(channel_id) {
                                e.insert(evt_tx);
                            }
                        }

                        self.inner.handler.on_data_channel(data_channel).await;
                    }
                }

                let data_channels = self.inner.data_channel_events_tx.lock().await;
                if let Some(evt_tx) = data_channels.get(&channel_id) {
                    let result = match evt {
                        RTCDataChannelEvent::OnOpen(_) => evt_tx.try_send(DataChannelEvent::OnOpen),
                        RTCDataChannelEvent::OnError(_) => {
                            evt_tx.try_send(DataChannelEvent::OnError)
                        }
                        RTCDataChannelEvent::OnClosing(_) => {
                            evt_tx.try_send(DataChannelEvent::OnClosing)
                        }
                        RTCDataChannelEvent::OnClose(_) => {
                            evt_tx.try_send(DataChannelEvent::OnClose)
                        }
                        RTCDataChannelEvent::OnBufferedAmountLow(_) => {
                            evt_tx.try_send(DataChannelEvent::OnBufferedAmountLow)
                        }
                        RTCDataChannelEvent::OnBufferedAmountHigh(_) => {
                            evt_tx.try_send(DataChannelEvent::OnBufferedAmountHigh)
                        }
                    };
                    if let Err(err) = result {
                        error!(
                            "Failed to send RTCDataChannelEvent to data channel {}: {:?}",
                            channel_id, err
                        );
                    }
                } else {
                    error!(
                        "Failed to get data_channel: {:?} for RTCDataChannelEvent",
                        channel_id
                    );
                }
            }
            RTCPeerConnectionEvent::OnTrack(evt) => {
                let track_id = match &evt {
                    RTCTrackEvent::OnOpen(init) => &init.track_id,
                    RTCTrackEvent::OnError(id) => id,
                    RTCTrackEvent::OnClosing(id) => id,
                    RTCTrackEvent::OnClose(id) => id,
                };

                let mut pending_on_track = None;

                if let RTCTrackEvent::OnOpen(init) = &evt {
                    let (id, track) = {
                        let mut core = self.inner.core.lock().await;
                        (
                            init.receiver_id.into(),
                            core.rtp_receiver(init.receiver_id).map(|receiver| {
                                let track = receiver.track();
                                MediaStreamTrack::new(
                                    track.stream_id().clone(),
                                    track.track_id().clone(),
                                    track.label().clone(),
                                    track.kind(),
                                    vec![],
                                )
                            }),
                        )
                    };

                    if let Some(track) = track {
                        // For simulcast, multiple RTCTrackEvent::OnOpen fire for the same
                        // track_id (one per RID as each layer's first RTP packet arrives).
                        // Only create the TrackRemote and call on_track the first time.
                        let already_open = self
                            .inner
                            .track_remote_events_tx
                            .lock()
                            .await
                            .contains_key(track_id);

                        if !already_open {
                            let (evt_tx, evt_rx) = channel(TRACK_REMOTE_EVENT_CHANNEL_CAPACITY);
                            let track_remote: Arc<dyn TrackRemote> =
                                Arc::new(TrackRemoteStaticRTP::new(
                                    track,
                                    init.receiver_id,
                                    self.inner.driver_event_tx.clone(),
                                    evt_rx,
                                ));

                            {
                                let mut rtp_transceivers = self.inner.rtp_transceivers.lock().await;
                                rtp_transceivers.entry(id).or_insert_with(|| {
                                    Arc::new(RtpTransceiverImpl::new(id, Arc::clone(&self.inner)))
                                });

                                let rtp_transceiver = rtp_transceivers.get(&id).unwrap();

                                let receiver: Arc<dyn RtpReceiver> =
                                    Arc::new(RtpReceiverImpl::new(
                                        id.into(),
                                        Arc::clone(&self.inner),
                                        Arc::clone(&track_remote),
                                    ));
                                rtp_transceiver.set_receiver(Some(receiver)).await;
                            }

                            self.inner
                                .track_remote_events_tx
                                .lock()
                                .await
                                .insert(track_id.clone(), (evt_tx, Arc::clone(&track_remote)));

                            pending_on_track = Some(track_remote);
                        }
                    }
                }

                let track_remote_entry = self
                    .inner
                    .track_remote_events_tx
                    .lock()
                    .await
                    .get(track_id)
                    .map(|(evt_tx, track_remote)| (evt_tx.clone(), Arc::clone(track_remote)));

                if let Some((evt_tx, track_remote)) = track_remote_entry {
                    let (track_id, result) = match evt {
                        RTCTrackEvent::OnOpen(init) => {
                            self.populate_track_remote_codings(
                                init.receiver_id,
                                init.ssrc,
                                &track_remote,
                            )
                            .await;
                            (
                                init.track_id.clone(),
                                evt_tx.try_send(TrackRemoteEvent::OnOpen(init)),
                            )
                        }
                        RTCTrackEvent::OnError(track_id) => {
                            (track_id, evt_tx.try_send(TrackRemoteEvent::OnError))
                        }
                        RTCTrackEvent::OnClosing(track_id) => {
                            (track_id, evt_tx.try_send(TrackRemoteEvent::OnEnding))
                        }
                        RTCTrackEvent::OnClose(track_id) => {
                            (track_id, evt_tx.try_send(TrackRemoteEvent::OnEnded))
                        }
                    };
                    if let Err(err) = result {
                        error!(
                            "Failed to send RTCTrackEvent to track remote {}: {:?}",
                            track_id, err
                        );
                    }
                } else {
                    error!(
                        "Failed to get track_remote: {:?} for RTCTrackEvent",
                        track_id
                    );
                }

                if let Some(track_remote) = pending_on_track {
                    self.inner.handler.on_track(track_remote).await;
                }
            }
        }
    }

    async fn handle_rtc_message(&mut self, message: RTCMessage) {
        match message {
            RTCMessage::DataChannelMessage(channel_id, dc_message) => {
                let data_channels = self.inner.data_channel_events_tx.lock().await;
                if let Some(evt_tx) = data_channels.get(&channel_id) {
                    if let Err(err) = evt_tx.try_send(DataChannelEvent::OnMessage(dc_message)) {
                        error!(
                            "Failed to send DataChannelMessage to data channel {}: {:?}",
                            channel_id, err
                        );
                    }
                } else {
                    error!(
                        "Failed to get data_channel: {:?} for DataChannelMessage",
                        channel_id
                    );
                }
            }
            RTCMessage::RtpPacket(track_id, packet) => {
                let track_remotes = self.inner.track_remote_events_tx.lock().await;
                if let Some(evt_tx) = track_remotes.get(&track_id) {
                    if let Err(err) = evt_tx.0.try_send(TrackRemoteEvent::OnRtpPacket(packet)) {
                        error!(
                            "Failed to send RtpPacket to track remote {}: {:?}",
                            track_id, err
                        );
                    }
                } else {
                    error!("Failed to get track_remote: {:?} for RtpPacket", track_id);
                }
            }
            RTCMessage::RtcpPacket(track_id, packets) => {
                let track_remotes = self.inner.track_remote_events_tx.lock().await;
                if let Some(evt_tx) = track_remotes.get(&track_id) {
                    if let Err(err) = evt_tx.0.try_send(TrackRemoteEvent::OnRtcpPacket(packets)) {
                        error!(
                            "Failed to send RtcpPacket to track remote {}: {:?}",
                            track_id, err
                        );
                    }
                } else {
                    error!("Failed to get track_remote: {:?} for RtcpPacket", track_id);
                }
            }
        }
    }

    async fn handle_driver_event(&mut self, evt: PeerConnectionDriverEvent) -> bool {
        match evt {
            PeerConnectionDriverEvent::SenderRtp(sender_id, packet) => {
                let mut core = self.inner.core.lock().await;
                if let Some(mut sender) = core.rtp_sender(sender_id) {
                    if let Err(err) = sender.write_rtp(packet) {
                        error!("Failed to send RTP: {}", err);
                    }
                } else {
                    error!(
                        "Failed to send RTP due to unknown sender id {:?}",
                        sender_id
                    );
                }
            }
            PeerConnectionDriverEvent::SenderRtcp(sender_id, rtcp_packets) => {
                let mut core = self.inner.core.lock().await;
                if let Some(mut sender) = core.rtp_sender(sender_id) {
                    if let Err(err) = sender.write_rtcp(rtcp_packets) {
                        error!("Failed to send RTCP: {}", err);
                    }
                } else {
                    error!(
                        "Failed to send RTCP feedback due to unknown sender id {:?}",
                        sender_id
                    );
                }
            }
            PeerConnectionDriverEvent::ReceiverRtcp(receiver_id, rtcp_packets) => {
                let mut core = self.inner.core.lock().await;
                if let Some(mut receiver) = core.rtp_receiver(receiver_id) {
                    if let Err(err) = receiver.write_rtcp(rtcp_packets) {
                        error!("Failed to send RTCP feedback: {}", err);
                    }
                } else {
                    error!(
                        "Failed to send RTCP feedback due to unknown receiver id {:?}",
                        receiver_id
                    );
                }
            }
            PeerConnectionDriverEvent::WriteNotify => {
                //Do nothing, just want to wake up from futures::select! in order to poll_write
            }
            PeerConnectionDriverEvent::IceGathering => {
                self.ice_gathering_active = true;
                self.stun_gathering_complete = false;
                self.turn_gathering_complete = false;

                // Gather TCP candidates
                let ice_gather_policy = {
                    let core = self.inner.core.lock().await;
                    core.get_configuration().ice_transport_policy()
                };

                if ice_gather_policy != RTCIceTransportPolicy::Relay {
                    for local_addr in self.tcp_transport.listeners.keys() {
                        // Gather passive TCP candidate
                        let passive_config = CandidateHostConfig {
                            base_config: CandidateConfig {
                                network: "tcp".to_owned(),
                                address: local_addr.ip().to_string(),
                                port: local_addr.port(),
                                component: 1,
                                ..Default::default()
                            },
                            tcp_type: rtc::ice::tcp_type::TcpType::Passive,
                        };
                        match passive_config.new_candidate_host() {
                            Ok(candidate) => {
                                if let Ok(candidate_init) =
                                    RTCIceCandidate::from(&candidate).to_json()
                                {
                                    trace!("TCP LocalIceCandidate Passive {:?}", candidate_init);
                                    let mut core = self.inner.core.lock().await;
                                    if let Err(err) = core.add_local_candidate(candidate_init) {
                                        error!(
                                            "Failed to add TCP passive local candidate: {}",
                                            err
                                        );
                                    }
                                }
                            }
                            Err(err) => {
                                error!("Failed to create TCP passive candidate: {}", err);
                            }
                        }

                        // Gather active TCP candidate
                        let active_config = CandidateHostConfig {
                            base_config: CandidateConfig {
                                network: "tcp".to_owned(),
                                address: local_addr.ip().to_string(),
                                port: 9, // Discard port placeholder for active candidates
                                component: 1,
                                ..Default::default()
                            },
                            tcp_type: rtc::ice::tcp_type::TcpType::Active,
                        };
                        match active_config.new_candidate_host() {
                            Ok(candidate) => {
                                if let Ok(candidate_init) =
                                    RTCIceCandidate::from(&candidate).to_json()
                                {
                                    trace!("TCP LocalIceCandidate Active {:?}", candidate_init);
                                    let mut core = self.inner.core.lock().await;
                                    if let Err(err) = core.add_local_candidate(candidate_init) {
                                        error!("Failed to add TCP active local candidate: {}", err);
                                    }
                                }
                            }
                            Err(err) => {
                                error!("Failed to create TCP active candidate: {}", err);
                            }
                        }
                    }
                }

                if self.stun_gatherer.state() != RTCIceGatheringState::Gathering {
                    if let Err(err) = self.stun_gatherer.gather().await {
                        error!("Failed to gather ice gathering: {}", err);
                    }
                }
                if self.turn_relayer.state() != RTCIceGatheringState::Gathering {
                    if let Err(err) = self.turn_relayer.gather().await {
                        error!("Failed to gather relay candidates: {}", err);
                    }
                }
            }
            PeerConnectionDriverEvent::RemoteIceTcpPassiveCandidate(candidate) => {
                if candidate.network_type().is_tcp()
                    && candidate.tcp_type() == rtc::ice::tcp_type::TcpType::Passive
                {
                    if let Ok(ip) = candidate.address().parse::<std::net::IpAddr>() {
                        let remote_addr = std::net::SocketAddr::new(ip, candidate.port());
                        let runtime = self.inner.runtime.clone();
                        let tx = self.inner.driver_event_tx.clone();
                        self.inner.runtime.spawn(Box::pin(async move {
                            trace!("Initiating TCP connect to {:?}", remote_addr);
                            match runtime.connect_tcp(remote_addr).await {
                                Ok(stream) => {
                                    let local_addr = stream
                                        .local_addr()
                                        .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
                                    let peer_addr = stream.peer_addr().unwrap_or(remote_addr);
                                    let four_tuple = FourTuple {
                                        local_addr,
                                        peer_addr,
                                    };
                                    let _ = tx
                                        .send(PeerConnectionDriverEvent::IncomingTcpStream(
                                            four_tuple, stream,
                                        ))
                                        .await;
                                }
                                Err(err) => {
                                    error!("Failed to connect TCP to {:?}: {}", remote_addr, err);
                                }
                            }
                        }));
                    }
                }
            }
            PeerConnectionDriverEvent::IncomingTcpStream(_, _) => {
                // Handled directly in event_loop select loop to avoid borrow mutability conflicts on tcp_read_futures
            }
            PeerConnectionDriverEvent::Close => {
                if let Err(err) = self.turn_relayer.close() {
                    error!("Failed to close turn_relayer: {}", err);
                }
                return true;
            }
        }

        false
    }

    async fn populate_track_remote_codings(
        &self,
        receiver_id: RTCRtpReceiverId,
        ssrc: u32,
        track_remote: &Arc<dyn TrackRemote>,
    ) {
        let codings = {
            let mut core = self.inner.core.lock().await;
            core.rtp_receiver(receiver_id).map(|receiver| {
                receiver
                    .track()
                    .codings()
                    .iter()
                    .filter(|coding| {
                        coding
                            .rtp_coding_parameters
                            .ssrc
                            .is_some_and(|coding_ssrc| coding_ssrc == ssrc)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
        };

        if let Some(codings) = codings {
            let mut existing_ssrcs = track_remote.ssrcs().await;
            for coding in codings {
                if let Some(coding_ssrc) = coding.rtp_coding_parameters.ssrc
                    && !existing_ssrcs.contains(&coding_ssrc)
                {
                    track_remote.add_coding(coding).await;
                    existing_ssrcs.push(coding_ssrc);
                }
            }
        }
    }
}
