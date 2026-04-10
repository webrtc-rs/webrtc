//! Peer connection driver (event loop)
//!
//! Follows the rtc EventLoop pattern with async select

#![allow(clippy::collapsible_if)]

use super::ice_gatherer::{RTCIceGatherer, RTCIceGathererEvent};
use crate::data_channel::{DataChannelEvent, DataChannelImpl};
use crate::media_stream::track_remote::static_rtp::TrackRemoteStaticRTP;
use crate::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use crate::peer_connection::PeerConnectionRef;
use crate::rtp_transceiver::rtp_receiver::RtpReceiverImpl;
use crate::rtp_transceiver::{RtpReceiver, RtpTransceiverImpl};
use crate::runtime::{
    AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, Receiver, Runtime, Sender,
    channel,
};
use bytes::BytesMut;
use futures::FutureExt;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{error, trace, warn};
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::event::{RTCDataChannelEvent, RTCPeerConnectionEvent, RTCTrackEvent};
use rtc::peer_connection::message::RTCMessage;
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::rtp_transceiver::{RTCRtpReceiverId, RTCRtpSenderId};
use rtc::sansio::Protocol;
use rtc::shared::error::{Error, Result};
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use rtc::{rtcp, rtp};
use rtc_shared::tcp_framing::{TcpFrameDecoder, frame_packet};
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
    WriteNotify,
    IceGathering,
    Close,
}

/// Inbound TCP packet + connection info — sent from per-connection read tasks into the driver loop
type TcpInbound = (SocketAddr, SocketAddr, BytesMut); // (local, peer, payload)

/// The driver for a peer connection
///
/// Runs the event loop following rtc's EventLoop pattern with select!
pub(crate) struct PeerConnectionDriver<I = NoopInterceptor>
where
    I: Interceptor,
{
    inner: Arc<PeerConnectionRef<I>>,
    /// ICE gatherer for managing ICE candidate gathering
    ice_gatherer: RTCIceGatherer,
    sockets: HashMap<SocketAddr, Arc<dyn AsyncUdpSocket>>,
    /// TCP: per-connection write channels keyed by (local_addr, peer_addr)
    tcp_write_txs: HashMap<(SocketAddr, SocketAddr), Sender<Vec<u8>>>,
    /// TCP: inbound packet channel — all TCP read tasks send here
    tcp_inbound_tx: Sender<TcpInbound>,
    tcp_inbound_rx: Receiver<TcpInbound>,
    /// TCP: new accepted connection channel — accept tasks send here
    tcp_new_conn_tx: Sender<Arc<dyn AsyncTcpStream>>,
    tcp_new_conn_rx: Receiver<Arc<dyn AsyncTcpStream>>,
    /// Async runtime — needed to spawn per-connection tasks
    runtime: Arc<dyn Runtime>,
    /// Handles for all background TCP tasks (accept loops + per-connection read/write).
    /// Stored so they can be aborted on clean shutdown rather than leaking.
    tcp_task_handles: Vec<JoinHandle>,
}

impl<I> Drop for PeerConnectionDriver<I>
where
    I: Interceptor,
{
    fn drop(&mut self) {
        self.abort_tcp_tasks();
    }
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
        tcp_listeners: Vec<Arc<dyn AsyncTcpListener>>,
        runtime: Arc<dyn Runtime>,
    ) -> Result<Self> {
        if sockets.is_empty() && tcp_listeners.is_empty() {
            return Err(Error::Other("no sockets available".to_owned()));
        }

        let (tcp_inbound_tx, tcp_inbound_rx) = channel::<TcpInbound>(256);
        let (tcp_new_conn_tx, tcp_new_conn_rx) = channel::<Arc<dyn AsyncTcpStream>>(32);

        // Spawn an accept loop for each passive TCP listener; store handles for clean shutdown.
        let mut tcp_task_handles = Vec::new();
        for listener in tcp_listeners {
            let new_conn_tx = tcp_new_conn_tx.clone();
            let handle = runtime.spawn(Box::pin(async move {
                loop {
                    match listener.accept().await {
                        Ok(stream) => {
                            match new_conn_tx.try_send(stream) {
                                Ok(()) => {}
                                Err(crate::runtime::TrySendError::Full(_)) => {
                                    // Channel is full (backpressure) — drop
                                    // the accepted stream but keep accepting.
                                    warn!(
                                        "TCP new-connection channel full; dropping accepted stream"
                                    );
                                }
                                Err(crate::runtime::TrySendError::Disconnected(_)) => {
                                    // Receiver dropped — driver shut down.
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            // Transient accept errors (e.g. EMFILE, ECONNABORTED)
                            // should not kill the listener permanently.
                            warn!("TCP accept error (continuing): {}", e);
                        }
                    }
                }
            }));
            tcp_task_handles.push(handle);
        }

        Ok(Self {
            inner,
            ice_gatherer,
            sockets,
            tcp_write_txs: HashMap::new(),
            tcp_inbound_tx,
            tcp_inbound_rx,
            tcp_new_conn_tx,
            tcp_new_conn_rx,
            runtime,
            tcp_task_handles,
        })
    }

    /// Register a new TCP connection (accepted or dialed) and spawn its read task
    fn register_tcp_connection(&mut self, stream: Arc<dyn AsyncTcpStream>) {
        let local_addr = match stream.local_addr() {
            Ok(a) => a,
            Err(e) => {
                error!("TCP stream local_addr: {}", e);
                return;
            }
        };
        let peer_addr = match stream.peer_addr() {
            Ok(a) => a,
            Err(e) => {
                error!("TCP stream peer_addr: {}", e);
                return;
            }
        };

        let key = (local_addr, peer_addr);
        if self.tcp_write_txs.contains_key(&key) {
            return; // already registered
        }

        let (write_tx, mut write_rx) = channel::<Vec<u8>>(64);
        self.tcp_write_txs.insert(key, write_tx);

        // Read task: decode RFC 4571 frames and forward to driver
        let read_stream = stream.clone();
        let inbound_tx = self.tcp_inbound_tx.clone();
        let read_handle = self.runtime.spawn(Box::pin(async move {
            let mut decoder = TcpFrameDecoder::new();
            let mut buf = vec![0u8; 4096];
            loop {
                match read_stream.read(&mut buf).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        decoder.extend_from_slice(&buf[..n]);
                        while let Some(packet) = decoder.next_packet() {
                            match inbound_tx.try_send((
                                local_addr,
                                peer_addr,
                                BytesMut::from(packet.as_slice()),
                            )) {
                                Ok(()) => {}
                                Err(crate::runtime::TrySendError::Full(_)) => {
                                    // Backpressure — drop the packet but keep reading.
                                    warn!(
                                        "TCP inbound channel full ({}->{}); dropping packet",
                                        local_addr, peer_addr
                                    );
                                }
                                Err(crate::runtime::TrySendError::Disconnected(_)) => {
                                    // Receiver dropped — driver shut down; stop.
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        trace!("TCP read error ({}→{}): {}", local_addr, peer_addr, e);
                        break;
                    }
                }
            }
        }));
        self.tcp_task_handles.push(read_handle);

        // Write task: receive framed bytes and write to stream
        let write_handle = self.runtime.spawn(Box::pin(async move {
            while let Some(data) = write_rx.recv().await {
                if let Err(e) = stream.write_all(&data).await {
                    trace!("TCP write error: {}", e);
                    break;
                }
            }
        }));
        self.tcp_task_handles.push(write_handle);
    }

    /// Abort all background TCP tasks spawned by this driver.
    fn abort_tcp_tasks(&mut self) {
        for handle in self.tcp_task_handles.drain(..) {
            handle.abort();
        }
    }

    /// Run the driver event loop
    ///
    /// This follows rtc Event Loop pattern exactly with select!
    pub(crate) async fn event_loop(
        &mut self,
        mut driver_event_rx: Receiver<PeerConnectionDriverEvent>,
    ) -> Result<()> {
        log::debug!("PeerConnectionDriver: event loop started");

        // Collect socket info into a vec for indexed access
        let socket_list: Vec<(SocketAddr, Arc<dyn AsyncUdpSocket>)> = self
            .sockets
            .iter()
            .map(|(addr, sock)| (*addr, sock.clone()))
            .collect();

        // Pre-allocate buffers once - one per socket, these will be reused forever
        let mut socket_buffers: Vec<Vec<u8>> =
            socket_list.iter().map(|_| vec![0u8; 2000]).collect();

        // Helper function to create a recv future for a specific socket.
        // Always returns (result, local_addr, idx, buf) so the caller can
        // re-queue the future even after a transient error.
        let create_socket_recv_future = |idx: usize,
                                         local_addr: SocketAddr,
                                         socket: Arc<dyn AsyncUdpSocket>,
                                         mut buf: Vec<u8>| async move {
            let result = socket.recv_from(&mut buf).await;
            (result, local_addr, idx, buf)
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

        // In TCP-only mode (no UDP sockets), FuturesUnordered is empty and
        // `.next()` would resolve to None immediately, causing a busy-loop
        // that starves the TCP branches. Insert a future that never resolves
        // so select! blocks on the TCP channels instead.
        if socket_list.is_empty() {
            socket_recv_futures.push(futures::future::pending().boxed());
        }

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

            // Runtime-agnostic select!
            futures::select! {
                // Timer expired
                _ = timer.fuse() => {
                    let now = Instant::now();
                    self.ice_gatherer.handle_timeout(now)?;
                    let mut core = self.inner.core.lock().await;
                    core.handle_timeout(now)?;
                }

                // Driver events (RTP, RTCP, or ICE candidate)
                evt = driver_event_rx.recv().fuse() => {
                    if let Some(evt) = evt {
                        let is_closed = self.handle_driver_event(evt).await;
                        if is_closed {
                            log::debug!("PeerConnectionDriver: clean shutdown");
                            self.abort_tcp_tasks();
                            return Ok(());
                        }
                    }
                }

                // Incoming network packet from any UDP socket
                result = socket_recv_futures.next().fuse() => {
                    match result {
                        Some((Ok((n, peer_addr)), local_addr, idx, buf)) => {
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
                        Some((Err(err), local_addr, idx, buf)) => {
                            // Transient OS errors (EAGAIN / EWOULDBLOCK / EINTR) occur
                            // spuriously on non-blocking sockets — re-queue the future
                            // and let the async runtime reschedule it.
                            // Fatal errors terminate the driver.
                            match err.kind() {
                                std::io::ErrorKind::WouldBlock
                                | std::io::ErrorKind::Interrupted => {
                                    trace!("Transient socket recv error (retrying): {}", err);
                                    let (_, socket) = &socket_list[idx];
                                    socket_recv_futures.push(
                                        create_socket_recv_future(idx, local_addr, socket.clone(), buf).boxed()
                                    );
                                }
                                _ => {
                                    error!("Fatal socket recv error: {}", err);
                                    self.abort_tcp_tasks();
                                    return Err(err.into());
                                }
                            }
                        }
                        None => {
                            // All UDP recv futures completed unexpectedly.
                            // TCP-only mode uses a pending() future so this
                            // branch is only reachable when UDP sockets exist.
                            self.abort_tcp_tasks();
                            return Err(Error::Other("all socket futures completed".to_owned()));
                        }
                    }
                }

                // New accepted TCP connection
                stream = self.tcp_new_conn_rx.recv().fuse() => {
                    if let Some(stream) = stream {
                        self.register_tcp_connection(stream);
                    }
                }

                // Decoded TCP frame from a read task
                pkt = self.tcp_inbound_rx.recv().fuse() => {
                    if let Some((local_addr, peer_addr, payload)) = pkt {
                        trace!("TCP received {} bytes from {} to {}", payload.len(), peer_addr, local_addr);
                        if let Err(err) = self.handle_read(TaggedBytesMut {
                            now: Instant::now(),
                            transport: TransportContext {
                                local_addr,
                                peer_addr,
                                ecn: None,
                                transport_protocol: TransportProtocol::TCP,
                            },
                            message: payload,
                        }).await {
                            error!("TCP handle_read error: {}", err);
                        }
                    }
                }
            }
        }
    }

    async fn handle_write(&mut self, msg: TaggedBytesMut) {
        match msg.transport.transport_protocol {
            TransportProtocol::UDP => {
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
            TransportProtocol::TCP => {
                let mut key = (msg.transport.local_addr, msg.transport.peer_addr);
                if !self.tcp_write_txs.contains_key(&key) {
                    // Active TCP: dial out on first outbound packet.
                    // The OS assigns the actual local address, so update `key`
                    // to match what register_tcp_connection() stored.
                    let peer_addr = msg.transport.peer_addr;
                    match self.runtime.connect_tcp(peer_addr).await {
                        Ok(stream) => {
                            // Update key to the actual (local, peer) pair so the
                            // lookup below finds the sender we just registered.
                            if let (Ok(local), Ok(peer)) = (stream.local_addr(), stream.peer_addr())
                            {
                                key = (local, peer);
                            }
                            self.register_tcp_connection(stream);
                        }
                        Err(e) => {
                            error!("TCP connect to {} failed: {}", peer_addr, e);
                            return;
                        }
                    }
                }
                if let Some(tx) = self.tcp_write_txs.get(&key) {
                    let framed = frame_packet(&msg.message);
                    if let Err(err) = tx.try_send(framed) {
                        match err {
                            crate::runtime::TrySendError::Full(_) => {
                                warn!(
                                    "TCP write channel full for {}->{}; keeping connection open",
                                    key.0, key.1
                                );
                            }
                            crate::runtime::TrySendError::Disconnected(_) => {
                                warn!(
                                    "TCP write channel closed for {}->{}; removing connection",
                                    key.0, key.1
                                );
                                self.tcp_write_txs.remove(&key);
                            }
                        }
                    }
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
                                let rtp_transceiver = rtp_transceivers
                                    .entry(id)
                                    .or_insert_with(|| {
                                        Arc::new(RtpTransceiverImpl::new(
                                            id,
                                            Arc::clone(&self.inner),
                                        ))
                                    })
                                    .clone();

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
                if self.ice_gatherer.state() != RTCIceGatheringState::Gathering {
                    if let Err(err) = self.ice_gatherer.gather().await {
                        error!("Failed to gather ice gathering: {}", err);
                    }
                }
            }
            PeerConnectionDriverEvent::Close => return true,
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
