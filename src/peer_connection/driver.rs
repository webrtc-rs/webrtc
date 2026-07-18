//! Peer connection driver (event loop)
//!
//! Follows the rtc EventLoop pattern with async select

use super::transports::stun_gatherer::{
    RTCStunGatherEventIn, RTCStunGatherEventOut, RTCStunGatherer,
};
use super::transports::turn_relayer::{RTCTurnRelayEventIn, RTCTurnRelayEventOut, RTCTurnRelayer};
use crate::data_channel::{DataChannelEvent, DataChannelImpl};
use crate::media_stream::track_local::TrackLocalEvent;
use crate::media_stream::track_remote::static_rtp::TrackRemoteStaticRTP;
use crate::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use crate::peer_connection::PeerConnectionRef;
use crate::peer_connection::transports::tcp_transport::RTCTcpTransport;
use crate::peer_connection::transports::{SocketRecvResult, is_retryable_socket_recv_error};
use crate::rtp_transceiver::rtp_receiver::RtpReceiverImpl;
use crate::rtp_transceiver::{RtpReceiver, RtpTransceiverImpl};
use crate::runtime::{AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, Receiver, channel};
use bytes::BytesMut;
use futures::FutureExt; // For .fuse() in futures::select!
use futures::future::OptionFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{error, trace, warn};
use rtc::ice::candidate::Candidate;
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::mdns::MDNS_PORT;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCIceTransportPolicy;
use rtc::peer_connection::event::{RTCDataChannelEvent, RTCPeerConnectionEvent, RTCTrackEvent};
use rtc::peer_connection::message::RTCMessage;
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::rtp_transceiver::{RTCRtpReceiverId, RTCRtpSenderId};
use rtc::sansio::Protocol;
use rtc::shared::error::{Error, Result};
use rtc::shared::{FourTuple, TaggedBytesMut, TransportContext, TransportProtocol};
use rtc::{rtcp, rtp};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Capacity of the internal driver event channel (WriteNotify, IceGathering, Close, …).
pub(crate) const PEER_CONNECTION_DRIVER_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Capacity of each data-channel event channel (OnOpen, OnMessage, OnClose, …).
pub(crate) const DATA_CHANNEL_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Capacity of each track-remote event channel (OnMute, OnUnmute, OnEnded, OnRtpPacket, OnRtcpPacket, …).
pub(crate) const TRACK_REMOTE_EVENT_CHANNEL_CAPACITY: usize = 256;
pub(crate) const TRACK_LOCAL_EVENT_CHANNEL_CAPACITY: usize = 256;

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(86400); // 1 day duration
const UDP_RECV_BUF_LEN: usize = 2000;

/// Upper bound on the number of datagrams the kernel may coalesce into one UDP GRO
/// receive (`UDP_SEGMENT`/GRO cap is 64 per buffer).
const MAX_GRO_SEGMENTS: usize = 64;

/// Upper bound on datagrams coalesced into one UDP GSO send. The kernel caps
/// `UDP_SEGMENT` at 64 segments per `sendmsg`; a socket may report fewer.
const MAX_GSO_SEGMENTS: usize = 64;

/// Upper bound on the total bytes of one UDP GSO batch. Kept at the single-datagram
/// UDP payload limit (65535) so a batch never trips the kernel's aggregate-size
/// checks and disables GSO — at ~1.25 KB datagrams this still coalesces ~50 per call.
const MAX_GSO_BATCH_BYTES: usize = 65535;

/// Minimum datagrams in a run before it is worth a single GSO `sendmsg` instead of
/// individual `send_to`s. GSO trades N cheap `sendto` syscalls for one heavier
/// `sendmsg` (control-message construction + kernel GSO setup) plus one buffer
/// concatenation, so it only pays off once the run is large. Below this the batching
/// machinery is pure overhead — exactly the paced single-connection case, where the
/// watermark dribbles a few datagrams per flush. A too-low threshold there GSOs the
/// occasional large drain and thrashes the tiny working set (measured on loopback:
/// threshold 2 → wall +58%, threshold 8 → +21%, threshold 16 → −15% i.e. back to a
/// win). Throughput-bound bursts (bulk/flood/many-connection) run far larger (50+),
/// so 16 keeps their full win (N=10 wall −34%, flood +77%) while erasing the
/// single-connection regression.
const MIN_GSO_RUN: usize = 16;

/// Per-datagram size assumed when sizing a GRO receive buffer, at the standard
/// Ethernet MTU. GRO coalesces up to `max_gro_segments()` datagrams into one buffer,
/// each at most one wire MTU, so the buffer must be `max_gro_segments() *
/// GRO_RECV_SEGMENT_LEN` — the kernel truncates (silently drops the tail datagrams)
/// if the coalesced super-datagram overflows the buffer. WebRTC keeps its own
/// datagrams well under this (DTLS/SCTP MTU ~1200); the 1500 headroom covers a peer
/// sending up to standard-MTU-sized datagrams. Jumbo-frame paths (MTU > 1500) are not
/// supported for GRO and would truncate.
const GRO_RECV_SEGMENT_LEN: usize = 1500;

/// Size a UDP receive buffer for a socket that may coalesce `max_gro` datagrams via
/// GRO. Falls back to the plain single-datagram size when GRO is unavailable.
///
/// NOTE: with GRO enabled this returns ~96 KB (64 * 1500) per socket vs the ~2 KB
/// non-GRO size — a real per-connection RSS cost that scales with socket count
/// (relevant at SFU scale). It cannot be shrunk without risking truncation (see
/// [`GRO_RECV_SEGMENT_LEN`]); the buffers are zero-initialized so pages stay unmapped
/// until actually written. Measured net effect is still an RSS *reduction* under load
/// because batching cuts per-packet allocator churn far more than the buffers cost.
fn gro_recv_buf_len(max_gro: usize) -> usize {
    if max_gro > 1 {
        max_gro.min(MAX_GRO_SEGMENTS) * GRO_RECV_SEGMENT_LEN
    } else {
        UDP_RECV_BUF_LEN
    }
}

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
    /// Reused scratch buffer for concatenating a run of same-destination datagrams
    /// into one UDP GSO send (see [`flush_writes`](Self::flush_writes)).
    gso_scratch: Vec<u8>,
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
            gso_scratch: Vec::new(),
            tcp_transport: RTCTcpTransport::new(tcp_listeners),
            ice_gathering_active: false,
            stun_gathering_complete: false,
            turn_gathering_complete: false,
        })
    }

    /// Mark the connection closing and wake any sender parked in send back-pressure.
    ///
    /// Called once the driver's [`event_loop`](Self::event_loop) has returned for ANY reason
    /// — a clean `close()`/`Drop` (where `closing` is already set) OR an abnormal error exit
    /// (a fatal SCTP/DTLS error on a timer tick, all UDP sockets gone, …), where nothing has
    /// set `closing`. Once the driver stops it no longer drains `outstanding_bytes` nor wakes
    /// `data_channel_backpressure`, so without this a blocking `send()` parked at the
    /// send-buffer limit would re-park forever. Setting `closing` makes the parked
    /// [`writable`](crate::data_channel::DataChannel::writable) loop return `ErrDataChannelClosed`
    /// on its next re-check; the wake makes that immediate. Idempotent on the clean path.
    pub(crate) fn signal_stopped(&self) {
        self.inner.closing.store(true, Ordering::Release);
        self.inner.data_channel_backpressure.notify_waiters();
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

        // Pre-allocate buffers once - one per socket, these will be reused forever.
        // Sized for the socket's GRO coalescing capacity so a single `recv_gro` can
        // hold up to `max_gro_segments()` datagrams without truncation.
        let mut udp_socket_buffers: Vec<Vec<u8>> = udp_socket_list
            .iter()
            .map(|(_, socket)| vec![0u8; gro_recv_buf_len(socket.max_gro_segments())])
            .collect();

        // Helper function to create a recv future for a specific socket. Uses GRO
        // (`recv_gro`) so one syscall may return several coalesced datagrams; `stride`
        // carries the per-datagram size for de-segmentation by the caller.
        let create_udp_recv_future = |idx: usize,
                                      local_addr: SocketAddr,
                                      socket: Arc<dyn AsyncUdpSocket>,
                                      mut buf: Vec<u8>| async move {
            match socket.recv_gro(&mut buf).await {
                Ok(gro) => SocketRecvResult::Packet {
                    n: gro.len,
                    stride: gro.stride,
                    local_addr,
                    peer_addr: gro.peer_addr,
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

        // Batch-drain: after one datagram wakes the select, non-blockingly drain a
        // bounded burst of additional ready datagrams from the same socket and feed
        // them all to handle_read before the next flush. Paired with the SCTP
        // handler's deferred flush, a burst of DATA coalesces into a single SACK and
        // amortizes the per-iteration cost (poll_writes/events/reads core locks,
        // timeout recompute, select setup).
        const MAX_UDP_RECV_BURST: usize = 64;
        // The burst buffer is shared across sockets, so size it for the largest GRO
        // capacity among them (falls back to the plain size when none support GRO).
        let burst_buf_len = udp_socket_list
            .iter()
            .map(|(_, socket)| gro_recv_buf_len(socket.max_gro_segments()))
            .max()
            .unwrap_or(UDP_RECV_BUF_LEN);
        let mut burst_buf = vec![0u8; burst_buf_len];

        loop {
            // Shutdown safety-net. `close()`/`Drop` set this flag and best-effort
            // wake the driver with a `Close` event. If that wake was dropped (a
            // momentarily full channel), this check still guarantees the loop —
            // and thus a dedicated reactor thread — terminates instead of leaking.
            if self.inner.closing.load(Ordering::Acquire) {
                if let Err(err) = self.turn_relayer.close() {
                    error!("Failed to close turn_relayer: {}", err);
                }
                return Ok(());
            }

            // Clear the coalescing write-flush gate BEFORE draining. `poll_writes`
            // drains the core unconditionally, so clearing here can never strand
            // data: a send that set the flag is either already enqueued (drained
            // this iteration) or enqueues a fresh `WriteNotify` for the next one.
            self.inner.write_pending.store(false, Ordering::Release);
            self.poll_writes().await?;
            self.poll_events().await;
            self.poll_reads().await?;

            // Wake senders blocked in `DataChannel::writable()`: the poll_* passes above
            // applied any SCTP buffer releases (acked/abandoned bytes) to the per-channel
            // `outstanding_bytes` counters, so a blocked `send()` can re-check and proceed.
            // Skipped entirely on the default unbounded path — `writable()` never parks when
            // the limit is `usize::MAX`, so there can be no waiter, and this keeps the
            // (throughput-sensitive) hot loop free of the per-iteration `Notify` lock.
            if self.inner.data_channel_send_buffer_limit != usize::MAX {
                self.inner.data_channel_backpressure.notify_waiters();
            }

            // 4.a poll next timeout
            let timeout = self.poll_timeout().await;
            let now = Instant::now();
            let delay_from_now = timeout.checked_duration_since(now).unwrap_or_default();

            // 4.b handle immediate timeout
            if delay_from_now.is_zero() {
                self.handle_timeout(now).await?;
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

            let tcp_accept_future: OptionFuture<_> =
                if !self.tcp_transport.accept_futures.is_empty() {
                    Some(self.tcp_transport.accept_futures.next())
                } else {
                    None
                }
                .into();
            futures::pin_mut!(tcp_accept_future);

            let tcp_read_future: OptionFuture<_> = if !self.tcp_transport.read_futures.is_empty() {
                Some(self.tcp_transport.read_futures.next())
            } else {
                None
            }
            .into();
            futures::pin_mut!(tcp_read_future);

            // Runtime-agnostic select!
            futures::select! {
                // Timer expired
                _ = timer.fuse() => {
                    self.handle_timeout(Instant::now()).await?;
                }

                // Driver events (RTP, RTCP, or ICE candidate)
                evt = driver_event_rx.recv().fuse() => {
                    if let Some(evt) = evt {
                        let is_closed = self.handle_driver_event(evt).await;
                        if is_closed {
                            trace!("Driver event channel closed, exiting event loop");
                            return Ok(());
                        }
                    }
                }

                // Incoming network packet from any udp socket
                udp_recv_result = udp_recv_future => {
                    if let Some(res) = udp_recv_result {
                        match res {
                            Some(SocketRecvResult::Packet { n, stride, local_addr, peer_addr, idx, buf }) => {
                                trace!("Received {} bytes from {} to {}", n, peer_addr, local_addr);

                                // A single recv may return several GRO-coalesced
                                // datagrams; split `buf[..n]` back into individual
                                // datagrams by `stride` and deliver each.
                                self.deliver_udp_batch(&buf, n, stride, local_addr, peer_addr).await;

                                // Immediately create a new future for this socket and reuse the buffer
                                let (socket_local_addr, socket) = &udp_socket_list[idx];
                                let socket_local_addr = *socket_local_addr;
                                let socket = socket.clone();
                                udp_recv_futures.push(
                                    create_udp_recv_future(idx, socket_local_addr, socket.clone(), buf).boxed()
                                );

                                // Batch-drain: drain a bounded burst of additional
                                // ready datagrams from this socket without blocking.
                                let mut burst = 0;
                                while burst < MAX_UDP_RECV_BURST {
                                    match socket.recv_gro(&mut burst_buf).now_or_never() {
                                        Some(Ok(gro)) => {
                                            self.deliver_udp_batch(&burst_buf, gro.len, gro.stride, socket_local_addr, gro.peer_addr).await;
                                            burst += 1;
                                        }
                                        _ => break, // would-block (pending) or error
                                    }
                                }
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

                                if active_socket_count == 0 && self.tcp_transport.is_empty() {
                                    return Err(err.into());
                                }
                            }
                            None => {
                                // All socket futures completed (should never happen in normal operation)
                                if self.tcp_transport.is_empty() {
                                    return Err(Error::Other("all socket futures completed".to_owned()));
                                }
                            }
                        }
                    }
                }

                tcp_accept_result = tcp_accept_future => {
                    if let Some(Some((local_addr, res))) = tcp_accept_result {
                        self.tcp_transport.on_accept(local_addr, res);
                    }
                }

                // Incoming TCP frame data from any tcp stream
                tcp_read_result = tcp_read_future => {
                    if let Some(Some(res) ) = tcp_read_result {
                        let packets = self.tcp_transport.on_read(res);
                        for packet in packets {
                            if let Err(err) = self.handle_read(packet).await {
                                error!("handle_read error on TCP: {}", err);
                            }
                        }
                    }
                }
            }
        }
    }

    async fn handle_write(&mut self, msg: TaggedBytesMut) -> Result<usize> {
        if msg.transport.transport_protocol == TransportProtocol::TCP {
            self.tcp_transport.write(&msg).await
        } else if msg.transport.peer_addr.port() == MDNS_PORT {
            if let Some(socket) = &self.mdns_socket {
                Ok(socket
                    .send_to(&msg.message, msg.transport.peer_addr)
                    .await?)
            } else {
                trace!(
                    "None mDNS socket, drop the packet to {:?} from {:?}",
                    msg.transport.peer_addr, msg.transport.local_addr
                );
                Ok(0)
            }
        } else if self
            .turn_relayer
            .contains_local_addr(msg.transport.local_addr)
        {
            let n = msg.message.len();
            self.turn_relayer.handle_write(msg)?;
            Ok(n)
        } else if let Some(udp_socket) = self.udp_sockets.get(&msg.transport.local_addr) {
            Ok(udp_socket
                .send_to(&msg.message, msg.transport.peer_addr)
                .await?)
        } else {
            warn!(
                "None tcp/udp socket, drop the packet to {:?} from {:?} for {:?}",
                msg.transport.peer_addr, msg.transport.local_addr, msg.transport.transport_protocol
            );
            Ok(0)
        }
    }

    /// Split a (possibly GRO-coalesced) UDP receive buffer into individual datagrams
    /// and feed each to [`handle_read`](Self::handle_read).
    ///
    /// `buf[..n]` holds one or more datagrams of `stride` bytes each (the last may be
    /// shorter). When `stride == n` (no GRO, or a lone datagram) this delivers exactly
    /// one datagram — identical to the pre-GRO behavior. A zero-length datagram
    /// (`n == 0`) is dropped (the loop never runs); empty UDP datagrams carry no
    /// STUN/DTLS/SCTP payload, so this is harmless.
    async fn deliver_udp_batch(
        &mut self,
        buf: &[u8],
        n: usize,
        stride: usize,
        local_addr: SocketAddr,
        peer_addr: SocketAddr,
    ) {
        let step = stride.max(1);
        let mut off = 0;
        while off < n {
            let end = (off + step).min(n);
            if let Err(err) = self
                .handle_read(TaggedBytesMut {
                    now: Instant::now(),
                    transport: TransportContext {
                        local_addr,
                        peer_addr,
                        ecn: None,
                        transport_protocol: TransportProtocol::UDP,
                    },
                    message: BytesMut::from(&buf[off..end]),
                })
                .await
            {
                error!("handle_read error: {}", err);
            }
            off = end;
        }
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
                            Self::populate_track_remote_codings(
                                self.inner.clone(),
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
                // RTCP about a *received* track goes to its TrackRemote; RTCP about a *sent*
                // track (feedback from the remote — RR/PLI/FIR — tagged with the sender's
                // track id) goes to its TrackLocal.
                let remote_tx = self
                    .inner
                    .track_remote_events_tx
                    .lock()
                    .await
                    .get(&track_id)
                    .map(|(evt_tx, _)| evt_tx.clone());
                if let Some(evt_tx) = remote_tx {
                    if let Err(err) = evt_tx.try_send(TrackRemoteEvent::OnRtcpPacket(packets)) {
                        error!(
                            "Failed to send RtcpPacket to track remote {}: {:?}",
                            track_id, err
                        );
                    }
                    return;
                }

                let local_tx = self
                    .inner
                    .track_local_events_tx
                    .lock()
                    .await
                    .get(&track_id)
                    .cloned();
                if let Some(evt_tx) = local_tx {
                    if let Err(err) = evt_tx.try_send(TrackLocalEvent::OnRtcpPacket(packets)) {
                        error!(
                            "Failed to send RtcpPacket to track local {}: {:?}",
                            track_id, err
                        );
                    }
                } else {
                    error!("Failed to route RtcpPacket: no track for {:?}", track_id);
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
                // Coalesced write-flush poke: wake up so the next loop iteration's
                // poll_writes drains the core. The `write_pending` gate (cleared at
                // the top of the loop) ensures a burst of sends enqueues at most
                // one of these.
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
                    let candidates = self.tcp_transport.gather_candidates();
                    let mut core = self.inner.core.lock().await;
                    for candidate_init in candidates {
                        trace!("TCP LocalIceCandidate {:?}", candidate_init);
                        if let Err(err) = core.add_local_candidate(candidate_init) {
                            error!("Failed to add TCP local candidate: {}", err);
                        }
                    }
                }

                if self.stun_gatherer.state() != RTCIceGatheringState::Gathering
                    && let Err(err) = self.stun_gatherer.gather().await
                {
                    error!("Failed to gather ice gathering: {}", err);
                }
                if self.turn_relayer.state() != RTCIceGatheringState::Gathering
                    && let Err(err) = self.turn_relayer.gather().await
                {
                    error!("Failed to gather relay candidates: {}", err);
                }
            }
            PeerConnectionDriverEvent::RemoteIceTcpPassiveCandidate(candidate) => {
                RTCTcpTransport::connect(
                    &candidate,
                    self.inner.runtime.clone(),
                    self.inner.driver_event_tx.clone(),
                );
            }
            PeerConnectionDriverEvent::IncomingTcpStream(four_tuple, stream) => {
                trace!("TCP stream connection established: {:?}", four_tuple);
                self.tcp_transport.register_stream(four_tuple, stream);
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
        inner: Arc<PeerConnectionRef<I>>,
        receiver_id: RTCRtpReceiverId,
        ssrc: u32,
        track_remote: &Arc<dyn TrackRemote>,
    ) {
        let codings = {
            let mut core = inner.core.lock().await;
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

        let Some(codings) = codings else {
            return;
        };
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

    async fn drain_core_writes(inner: Arc<PeerConnectionRef<I>>) -> Vec<TaggedBytesMut> {
        let mut writes = Vec::new();
        let mut core = inner.core.lock().await;
        while let Some(msg) = core.poll_write() {
            writes.push(msg);
        }
        writes
    }

    async fn drain_core_events(inner: Arc<PeerConnectionRef<I>>) -> Vec<RTCPeerConnectionEvent> {
        let mut events = Vec::new();
        let mut core = inner.core.lock().await;
        while let Some(event) = core.poll_event() {
            events.push(event);
        }
        events
    }

    async fn drain_core_reads(inner: Arc<PeerConnectionRef<I>>) -> Vec<RTCMessage> {
        let mut messages = Vec::new();
        let mut core = inner.core.lock().await;
        while let Some(message) = core.poll_read() {
            messages.push(message);
        }
        messages
    }

    async fn poll_writes(&mut self) -> Result<()> {
        // 1.a stun_gatherer poll_write()
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

        // 1.b turn_relayer poll_write()
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

        // 1.c peer_connection poll_write() - Send all outgoing packets, coalescing
        // consecutive same-destination datagrams into single UDP GSO syscalls.
        let writes = Self::drain_core_writes(self.inner.clone()).await;
        self.flush_writes(writes).await;

        Ok(())
    }

    /// Send a drained batch of outgoing packets, coalescing maximal runs of
    /// consecutive datagrams sharing the same UDP `(local_addr, peer_addr, ecn)`
    /// into a single `UDP_SEGMENT` (GSO) syscall.
    ///
    /// During a bulk transfer the ICE handler stamps every non-STUN packet with the
    /// one selected candidate pair, so these runs are long and homogeneous (each an
    /// MTU-sized DTLS record → equal-size datagram) — the ideal GSO case. A run is
    /// extended while the next datagram has the same 4-tuple and is exactly
    /// `segment_size` bytes (a shorter datagram can only be the run's final segment,
    /// a larger one starts a fresh run), capped by the socket's GSO segment limit and
    /// [`MAX_GSO_BATCH_BYTES`]. Everything the GSO path can't own — TCP, mDNS,
    /// TURN-relayed, or datagrams for an unknown socket — falls back to the
    /// per-packet [`handle_write`](Self::handle_write) path unchanged.
    async fn flush_writes(&mut self, mut writes: Vec<TaggedBytesMut>) {
        // Borrow the reusable concat buffer out of `self` so the sends below don't
        // hold a `&self` borrow across `.await`.
        let mut scratch = std::mem::take(&mut self.gso_scratch);
        let n = writes.len();
        let mut i = 0;
        while i < n {
            let tp = writes[i].transport;
            let seg = writes[i].message.len();

            // Only plain UDP datagrams routed to one of our sockets are GSO-eligible.
            let plain_udp = tp.transport_protocol == TransportProtocol::UDP
                && tp.peer_addr.port() != MDNS_PORT
                && !self.turn_relayer.contains_local_addr(tp.local_addr)
                && self.udp_sockets.contains_key(&tp.local_addr);
            if !plain_udp {
                // TCP / mDNS / TURN-relayed / unknown-socket: owned per-packet path.
                // Move the message out (writes[i] is never read again) rather than
                // cloning it — this runs for every packet on a TURN-relayed connection,
                // so a per-packet deep copy here would be a real cost.
                let msg = TaggedBytesMut {
                    now: writes[i].now,
                    transport: writes[i].transport,
                    message: std::mem::take(&mut writes[i].message),
                };
                let four_tuple: FourTuple = FourTuple::from(&msg.transport);
                if let Err(err) = self.handle_write(msg).await {
                    error!(
                        "Failed to write packet to {:?} from {:?}: {}",
                        four_tuple.peer_addr, four_tuple.local_addr, err
                    );
                }
                i += 1;
                continue;
            }

            let socket = self.udp_sockets.get(&tp.local_addr).unwrap().clone();
            let ecn = tp.ecn.map(|e| e as u8);

            // Max datagrams the kernel accepts in one GSO `sendmsg` for this socket
            // (1 = GSO unavailable / empty first datagram → no batching).
            let max_seg = if seg > 0 {
                socket.max_gso_segments().min(MAX_GSO_SEGMENTS)
            } else {
                1
            };

            // Grow the GSO run [i, end) while the 4-tuple matches and the size rule holds.
            let mut end = i + 1;
            if max_seg > 1 {
                let mut total = seg;
                while (end - i) < max_seg && end < n {
                    let w_tp = writes[end].transport;
                    // Same 4-tuple (local, peer) already implies non-mDNS and
                    // non-TURN-relayed here (tp passed the plain_udp gate), so those two
                    // checks are not repeated; protocol/ecn still must match.
                    if w_tp.transport_protocol != TransportProtocol::UDP
                        || w_tp.peer_addr != tp.peer_addr
                        || w_tp.local_addr != tp.local_addr
                        || w_tp.ecn.map(|e| e as u8) != ecn
                    {
                        break;
                    }
                    let wl = writes[end].message.len();
                    // A larger datagram cannot be a GSO segment — it starts the next run.
                    if wl == 0 || wl > seg || total + wl > MAX_GSO_BATCH_BYTES {
                        break;
                    }
                    total += wl;
                    end += 1;
                    // A shorter datagram is only valid as the run's final segment.
                    if wl < seg {
                        break;
                    }
                }
            }

            // GSO only when the run is both worth it and physically batchable. Clamp the
            // threshold to `max_seg` so a socket with a small GSO limit (< MIN_GSO_RUN)
            // still batches rather than degrading to all-singleton sends.
            if max_seg > 1 && end - i >= MIN_GSO_RUN.min(max_seg) {
                // Large run: one GSO sendmsg beats end-i individual send_to syscalls.
                scratch.clear();
                for w in &writes[i..end] {
                    scratch.extend_from_slice(&w.message);
                }
                if let Err(err) = socket.send_segments(&scratch, seg, tp.peer_addr, ecn).await {
                    error!(
                        "Failed to GSO-send {} datagrams to {:?} from {:?}: {}",
                        end - i,
                        tp.peer_addr,
                        tp.local_addr,
                        err
                    );
                }
            } else {
                // Small run (or singleton): individual send_to is cheaper than the GSO
                // setup. (ECN is carried only on the GSO run path; inert today since the
                // rtc core always emits ecn: None.)
                for w in &writes[i..end] {
                    if let Err(err) = socket.send_to(&w.message, tp.peer_addr).await {
                        error!(
                            "Failed to write packet to {:?} from {:?}: {}",
                            tp.peer_addr, tp.local_addr, err
                        );
                    }
                }
            }
            i = end;
        }

        scratch.clear();
        self.gso_scratch = scratch;
    }

    async fn poll_events(&mut self) {
        // 2.a stun_gatherer poll_event()
        while let Some(event) = self.stun_gatherer.poll_event() {
            self.handle_stun_gather_event(event).await;
        }

        // 2.b turn_relayer poll_event()
        while let Some(event) = self.turn_relayer.poll_event() {
            self.handle_turn_relay_event(event).await;
        }

        // 2.c peer_connection poll_event() - Process all events
        for event in Self::drain_core_events(self.inner.clone()).await {
            self.handle_rtc_event(event).await;
        }
    }

    async fn poll_reads(&mut self) -> Result<()> {
        // 3.a turn_relayer poll_read() - deliver decapsulated relay data,
        // but no need for stun_gatherer poll_read()
        let mut turn_messages = Vec::new();
        while let Some(message) = self.turn_relayer.poll_read() {
            turn_messages.push(message);
        }
        if !turn_messages.is_empty() {
            let mut core = self.inner.core.lock().await;
            for message in turn_messages {
                core.handle_read(message)?;
            }
        }

        // 3.b peer_connection poll_read() - Process incoming messages
        for message in Self::drain_core_reads(self.inner.clone()).await {
            self.handle_rtc_message(message).await;
        }

        Ok(())
    }

    async fn poll_timeout(&mut self) -> Instant {
        let core_timeout = {
            let mut core = self.inner.core.lock().await;
            core.poll_timeout()
        };
        let stun_timeout = self.stun_gatherer.poll_timeout();
        let turn_timeout = self.turn_relayer.poll_timeout();

        [core_timeout, stun_timeout, turn_timeout]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or_else(|| Instant::now() + DEFAULT_TIMEOUT_DURATION)
    }

    async fn handle_timeout(&mut self, now: Instant) -> Result<()> {
        self.stun_gatherer.handle_timeout(now)?;
        self.turn_relayer.handle_timeout(now)?;
        let mut core = self.inner.core.lock().await;
        core.handle_timeout(now)?;
        Ok(())
    }
}

#[cfg(test)]
mod gro_buf_tests {
    use super::{GRO_RECV_SEGMENT_LEN, MAX_GRO_SEGMENTS, UDP_RECV_BUF_LEN, gro_recv_buf_len};

    #[test]
    fn gro_recv_buf_len_sizes_for_capacity_and_falls_back_without_gro() {
        // GRO available: sized to hold up to `max_gro` coalesced datagrams.
        assert_eq!(gro_recv_buf_len(64), 64 * GRO_RECV_SEGMENT_LEN);
        assert_eq!(gro_recv_buf_len(8), 8 * GRO_RECV_SEGMENT_LEN);
        // Capped at the kernel's max coalescing (MAX_GRO_SEGMENTS).
        assert_eq!(
            gro_recv_buf_len(1000),
            MAX_GRO_SEGMENTS * GRO_RECV_SEGMENT_LEN
        );
        // GRO unavailable (max_gro <= 1): plain single-datagram buffer.
        assert_eq!(gro_recv_buf_len(1), UDP_RECV_BUF_LEN);
    }
}
