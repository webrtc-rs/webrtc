//! Socket-level transports (UDP/TCP, STUN gathering, TURN relaying) **and** the W3C transport
//! objects an application reads through.
//!
//! Two layers share this module. Below: the plumbing that moves bytes — see `stun_gatherer`,
//! `tcp_transport`, `turn_relayer` and the GRO/GSO constants, all crate-private. Above:
//! [`SctpTransport`], [`DtlsTransport`] and [`IceTransport`], the public handles mirroring the
//! W3C interfaces of the same names.
//!
//! # The transport objects
//!
//! They are obtained by walking, exactly as the spec walks them — the entry points are
//! [`PeerConnection::sctp`] and a sender's or receiver's `transport()`:
//!
//! ```text
//! pc.sctp().await.expect("SCTP negotiated").transport().ice_transport()
//! sender.transport().await?.expect("sender associated")
//! ```
//!
//! ## Two things that differ from a browser
//!
//! **Identity is [`RTCTransportId`], not reference equality.** JavaScript answers "is this the
//! same transport?" with `===`. These handles are values over a shared core, so `Arc::ptr_eq`
//! would answer a different question — whether two *handles* are the same object, not whether
//! they name the same transport. Compare ids instead:
//!
//! ```text
//! sctp.transport().id() == sender.transport().await?.expect("sender associated").id()
//! ```
//!
//! **State is read, not delivered.** There is no `onstatechange`, `onerror` or
//! `onselectedcandidatepairchange`. ICE state remains observable through the existing
//! peer-connection events; DTLS and SCTP are poll-only.
//!
//! Those two are the differences you notice first. Eight more are smaller but can still surprise
//! — `maxMessageSize` is always finite, `sender.transport()` is null until its transceiver is
//! associated, `sctp()` never returns to `None` after a renegotiation that drops data, and
//! `RTCIceRole` never reports `"unknown"`. All ten are listed, with their reasons, in
//! [`docs/transport-objects.md`](https://github.com/webrtc-rs/webrtc/blob/master/docs/transport-objects.md).
//!
//! ## Why some methods are `async` and others are not
//!
//! This follows the IDL's nullability rather than a house style. `id()`, the two non-null edges
//! ([`SctpTransport::transport`], [`DtlsTransport::ice_transport`]) and
//! [`IceTransport::component`] answer from data the handle already holds, so they take no lock and
//! are not `async`. Everything that reads live transport state takes the lock, and so is.
//!
//! [`PeerConnection::sctp`]: crate::peer_connection::PeerConnection::sctp

use rtc::shared::FourTuple;
use std::io;
use std::net::SocketAddr;

pub(crate) mod stun_gatherer;
pub(crate) mod tcp_transport;
pub(crate) mod turn_relayer;

/// Plain single-datagram UDP receive buffer size (no GRO coalescing).
pub(crate) const UDP_RECV_BUF_LEN: usize = 2000;

/// Upper bound on the number of datagrams the kernel may coalesce into one UDP GRO
/// receive (`UDP_SEGMENT`/GRO cap is 64 per buffer).
pub(crate) const MAX_GRO_SEGMENTS: usize = 64;

/// Upper bound on datagrams coalesced into one UDP GSO send. The kernel caps
/// `UDP_SEGMENT` at 64 segments per `sendmsg`; a socket may report fewer.
pub(crate) const MAX_GSO_SEGMENTS: usize = 64;

/// Upper bound on the total bytes of one UDP GSO batch. Kept at the single-datagram
/// UDP payload limit (65535) so a batch never trips the kernel's aggregate-size
/// checks and disables GSO — at ~1.25 KB datagrams this still coalesces ~50 per call.
pub(crate) const MAX_GSO_BATCH_BYTES: usize = 65535;

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
pub(crate) const MIN_GSO_RUN: usize = 16;

/// Per-datagram size assumed when sizing a GRO receive buffer, at the standard
/// Ethernet MTU. GRO coalesces up to `max_gro_segments()` datagrams into one buffer,
/// each at most one wire MTU, so the buffer must be `max_gro_segments() *
/// GRO_RECV_SEGMENT_LEN` — the kernel truncates (silently drops the tail datagrams)
/// if the coalesced super-datagram overflows the buffer. WebRTC keeps its own
/// datagrams well under this (DTLS/SCTP MTU ~1200); the 1500 headroom covers a peer
/// sending up to standard-MTU-sized datagrams. Jumbo-frame paths (MTU > 1500) are not
/// supported for GRO and would truncate.
pub(crate) const GRO_RECV_SEGMENT_LEN: usize = 1500;

/// Size a UDP receive buffer for a socket that may coalesce `max_gro` datagrams via
/// GRO. Falls back to the plain single-datagram size when GRO is unavailable.
///
/// NOTE: with GRO enabled this returns ~96 KB (64 * 1500) per socket vs the ~2 KB
/// non-GRO size — a real per-connection RSS cost that scales with socket count
/// (relevant at SFU scale). It cannot be shrunk without risking truncation (see
/// [`GRO_RECV_SEGMENT_LEN`]); the buffers are zero-initialized so pages stay unmapped
/// until actually written. Measured net effect is still an RSS *reduction* under load
/// because batching cuts per-packet allocator churn far more than the buffers cost.
pub(crate) fn gro_recv_buf_len(max_gro: usize) -> usize {
    if max_gro > 1 {
        max_gro.min(MAX_GRO_SEGMENTS) * GRO_RECV_SEGMENT_LEN
    } else {
        UDP_RECV_BUF_LEN
    }
}

pub(crate) enum SocketRecvResult {
    Packet {
        /// Total bytes received into `buf` across all GRO-coalesced datagrams.
        n: usize,
        /// Per-datagram size for GRO de-segmentation; `buf[..n]` is walked in
        /// `stride`-sized steps (the last datagram may be shorter). Equals `n`
        /// when a single datagram was received.
        stride: usize,
        local_addr: SocketAddr,
        peer_addr: SocketAddr,
        idx: usize,
        buf: Vec<u8>,
    },
    Error {
        err: io::Error,
        local_addr: SocketAddr,
        idx: usize,
        buf: Vec<u8>,
    },
}

pub(crate) enum TcpReadResult {
    Packet {
        four_tuple: FourTuple,
        n: usize,
        buf: Vec<u8>,
    },
    Error {
        four_tuple: FourTuple,
        err: io::Error,
        buf: Vec<u8>,
    },
}

pub(crate) fn is_retryable_socket_recv_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::Interrupted
            | io::ErrorKind::WouldBlock
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::TimedOut
    )
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

// ---------------------------------------------------------------------------
// W3C transport objects
// ---------------------------------------------------------------------------

use crate::error::{Error, Result};
use crate::peer_connection::PeerConnectionRef;
use rtc::peer_connection::RTCPeerConnection;
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::peer_connection::transport::{
    RTCDtlsTransport as CoreDtlsTransport, RTCDtlsTransportState, RTCIceCandidate,
    RTCIceCandidatePair, RTCIceComponent, RTCIceParameters, RTCIceRole, RTCIceTransportState,
    RTCSctpTransportState, RTCTransportId,
};
use rtc::rtp_transceiver::{RTCRtpReceiverId, RTCRtpSenderId};
use std::sync::Arc;

/// The SCTP transport that carries a peer connection's data channels.
///
/// Obtained from [`PeerConnection::sctp`](crate::peer_connection::PeerConnection::sctp).
///
/// # Specification
///
/// * [W3C](https://www.w3.org/TR/webrtc/#dom-rtcsctptransport)
#[async_trait::async_trait]
pub trait SctpTransport: crate::sealed::Sealed + Send + Sync + 'static {
    /// This transport's identity. See [`RTCTransportId`].
    fn id(&self) -> RTCTransportId;

    /// The DTLS transport all SCTP packets for data channels are sent over.
    ///
    /// Never absent, and takes no lock: the spec types `transport` non-nullable.
    fn transport(&self) -> Arc<dyn DtlsTransport>;

    /// The current state of the SCTP association.
    async fn state(&self) -> Result<RTCSctpTransportState>;

    /// The maximum size, in bytes, of a message that may be sent on a data channel.
    ///
    /// The W3C type is `unrestricted double` so that an implementation with no limit can report
    /// positive infinity. This one always has a limit — the negotiated value also sizes a real
    /// buffer — so the value is always finite.
    async fn max_message_size(&self) -> Result<u32>;

    /// The maximum number of data channels that may be used simultaneously.
    ///
    /// `None` until the association reaches the connected state, at which point it is the smaller
    /// of the negotiated inbound and outbound stream counts.
    async fn max_channels(&self) -> Result<Option<u16>>;
}

/// The DTLS transport over which RTP, RTCP and SCTP are sent and received.
///
/// Obtained from [`SctpTransport::transport`], or from a sender's or receiver's `transport()`.
///
/// # Specification
///
/// * [W3C](https://www.w3.org/TR/webrtc/#dom-rtcdtlstransport)
#[async_trait::async_trait]
pub trait DtlsTransport: crate::sealed::Sealed + Send + Sync + 'static {
    /// This transport's identity. See [`RTCTransportId`].
    fn id(&self) -> RTCTransportId;

    /// The ICE transport this DTLS transport runs over.
    ///
    /// Never absent, and takes no lock — the spec types `iceTransport` non-nullable.
    fn ice_transport(&self) -> Arc<dyn IceTransport>;

    /// The current state of the DTLS connection.
    async fn state(&self) -> Result<RTCDtlsTransportState>;

    /// The peer's certificate chain, DER-encoded — the analogue of the browser's
    /// `sequence<ArrayBuffer>`.
    ///
    /// Empty until the handshake completes.
    async fn get_remote_certificates(&self) -> Result<Vec<Vec<u8>>>;
}

/// The ICE transport over which packets are sent and received.
///
/// Obtained from [`DtlsTransport::ice_transport`].
///
/// # Specification
///
/// * [W3C](https://www.w3.org/TR/webrtc/#dom-rtcicetransport)
#[async_trait::async_trait]
pub trait IceTransport: crate::sealed::Sealed + Send + Sync + 'static {
    /// This transport's identity. See [`RTCTransportId`].
    fn id(&self) -> RTCTransportId;

    /// The ICE component this transport carries.
    ///
    /// Always [`RTCIceComponent::Rtp`]: RTCP multiplexing is required, and the spec specifies
    /// `rtp` for a transport carrying both. A constant, so it takes no lock.
    fn component(&self) -> RTCIceComponent;

    /// Whether this agent is controlling or controlled.
    async fn role(&self) -> Result<RTCIceRole>;

    /// The current state of ICE connectivity.
    async fn state(&self) -> Result<RTCIceTransportState>;

    /// How far candidate gathering has progressed.
    ///
    /// The spec types this `RTCIceGathererState`, a second enum whose values are identical to
    /// [`RTCIceGatheringState`]'s; this crate carries one type for both.
    async fn gathering_state(&self) -> Result<RTCIceGatheringState>;

    /// The local candidates gathered so far.
    async fn get_local_candidates(&self) -> Result<Vec<RTCIceCandidate>>;

    /// The remote candidates received so far.
    async fn get_remote_candidates(&self) -> Result<Vec<RTCIceCandidate>>;

    /// The nominated candidate pair, or `None` until ICE selects one.
    async fn get_selected_candidate_pair(&self) -> Result<Option<RTCIceCandidatePair>>;

    /// The local ICE parameters, or `None` before a local description has supplied them.
    async fn get_local_parameters(&self) -> Result<Option<RTCIceParameters>>;

    /// The remote ICE parameters, or `None` before a remote description has supplied them.
    async fn get_remote_parameters(&self) -> Result<Option<RTCIceParameters>>;
}

/// The route a handle walks to find its transport in the core again.
///
/// The core's public surface is spec-shaped: `RTCPeerConnection` exposes `sctp()` and nothing
/// else, and everything below is reached by walking. A borrowed view cannot be held across an
/// `await`, so each async call re-walks from an entry point — and therefore has to remember which
/// one it came in through. A media-only connection has no `sctp()`, so `Sctp` is not always
/// available.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DtlsRoute {
    /// Reached through `pc.sctp()`.
    Sctp,
    /// Reached through a sender's `transport()`.
    Sender(RTCRtpSenderId),
    /// Reached through a receiver's `transport()`.
    Receiver(RTCRtpReceiverId),
}

impl DtlsRoute {
    /// Walks to the core's DTLS view and copies out `f`'s result, so nothing borrowed escapes the
    /// lock.
    fn with_dtls<T>(
        self,
        peer_connection: &mut RTCPeerConnection,
        f: impl FnOnce(CoreDtlsTransport<'_>) -> T,
    ) -> Option<T> {
        match self {
            DtlsRoute::Sctp => peer_connection.sctp().map(|sctp| f(sctp.transport())),
            DtlsRoute::Sender(id) => match peer_connection.rtp_sender(id) {
                Some(sender) => sender.transport().map(f),
                None => None,
            },
            DtlsRoute::Receiver(id) => match peer_connection.rtp_receiver(id) {
                Some(receiver) => receiver.transport().map(f),
                None => None,
            },
        }
    }
}

/// Concrete SCTP transport handle (generic over interceptor type).
pub(crate) struct SctpTransportImpl {
    id: RTCTransportId,
    dtls_id: RTCTransportId,
    ice_id: RTCTransportId,
    inner: Arc<PeerConnectionRef>,
}

impl SctpTransportImpl {
    pub(crate) fn new(
        id: RTCTransportId,
        dtls_id: RTCTransportId,
        ice_id: RTCTransportId,
        inner: Arc<PeerConnectionRef>,
    ) -> Self {
        Self {
            id,
            dtls_id,
            ice_id,
            inner,
        }
    }
}

impl crate::sealed::Sealed for SctpTransportImpl {}

#[async_trait::async_trait]
impl SctpTransport for SctpTransportImpl {
    fn id(&self) -> RTCTransportId {
        self.id
    }

    fn transport(&self) -> Arc<dyn DtlsTransport> {
        Arc::new(DtlsTransportImpl::new(
            self.dtls_id,
            self.ice_id,
            DtlsRoute::Sctp,
            Arc::clone(&self.inner),
        ))
    }

    async fn state(&self) -> Result<RTCSctpTransportState> {
        let peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .sctp()
            .ok_or(Error::ErrSCTPTransportNotExisted)?
            .state())
    }

    async fn max_message_size(&self) -> Result<u32> {
        let peer_connection = self.inner.core.lock().await;
        peer_connection
            .sctp()
            .ok_or(Error::ErrSCTPTransportNotExisted)?
            .max_message_size()
            // Unreachable in practice: `sctp()` is `Some` only once the transport has started,
            // which is the same point the negotiated size is recorded.
            .ok_or(Error::ErrSCTPTransportNotExisted)
    }

    async fn max_channels(&self) -> Result<Option<u16>> {
        let peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .sctp()
            .ok_or(Error::ErrSCTPTransportNotExisted)?
            .max_channels())
    }
}

/// Concrete DTLS transport handle (generic over interceptor type).
pub(crate) struct DtlsTransportImpl {
    id: RTCTransportId,
    ice_id: RTCTransportId,
    route: DtlsRoute,
    inner: Arc<PeerConnectionRef>,
}

impl DtlsTransportImpl {
    pub(crate) fn new(
        id: RTCTransportId,
        ice_id: RTCTransportId,
        route: DtlsRoute,
        inner: Arc<PeerConnectionRef>,
    ) -> Self {
        Self {
            id,
            ice_id,
            route,
            inner,
        }
    }
}

impl crate::sealed::Sealed for DtlsTransportImpl {}

#[async_trait::async_trait]
impl DtlsTransport for DtlsTransportImpl {
    fn id(&self) -> RTCTransportId {
        self.id
    }

    fn ice_transport(&self) -> Arc<dyn IceTransport> {
        Arc::new(IceTransportImpl::new(
            self.ice_id,
            self.route,
            Arc::clone(&self.inner),
        ))
    }

    async fn state(&self) -> Result<RTCDtlsTransportState> {
        let mut peer_connection = self.inner.core.lock().await;
        self.route
            .with_dtls(&mut peer_connection, |dtls| dtls.state())
            .ok_or(Error::ErrDTLSTransportNotExisted)
    }

    async fn get_remote_certificates(&self) -> Result<Vec<Vec<u8>>> {
        let mut peer_connection = self.inner.core.lock().await;
        self.route
            .with_dtls(&mut peer_connection, |dtls| {
                dtls.get_remote_certificates().to_vec()
            })
            .ok_or(Error::ErrDTLSTransportNotExisted)
    }
}

/// Concrete ICE transport handle (generic over interceptor type).
pub(crate) struct IceTransportImpl {
    id: RTCTransportId,
    route: DtlsRoute,
    inner: Arc<PeerConnectionRef>,
}

impl IceTransportImpl {
    pub(crate) fn new(id: RTCTransportId, route: DtlsRoute, inner: Arc<PeerConnectionRef>) -> Self {
        Self { id, route, inner }
    }
}

impl crate::sealed::Sealed for IceTransportImpl {}

/// Reads one value from the ICE transport, walking to it and copying the result out under the
/// lock.
macro_rules! read_ice {
    ($self:ident, |$ice:ident| $body:expr) => {{
        let mut peer_connection = $self.inner.core.lock().await;
        $self
            .route
            .with_dtls(&mut peer_connection, |dtls| {
                let $ice = dtls.ice_transport();
                $body
            })
            .ok_or(Error::ErrDTLSTransportNotExisted)
    }};
}

#[async_trait::async_trait]
impl IceTransport for IceTransportImpl {
    fn id(&self) -> RTCTransportId {
        self.id
    }

    fn component(&self) -> RTCIceComponent {
        RTCIceComponent::Rtp
    }

    async fn role(&self) -> Result<RTCIceRole> {
        read_ice!(self, |ice| ice.role())
    }

    async fn state(&self) -> Result<RTCIceTransportState> {
        read_ice!(self, |ice| ice.state())
    }

    async fn gathering_state(&self) -> Result<RTCIceGatheringState> {
        read_ice!(self, |ice| ice.gathering_state())
    }

    async fn get_local_candidates(&self) -> Result<Vec<RTCIceCandidate>> {
        read_ice!(self, |ice| ice.get_local_candidates())
    }

    async fn get_remote_candidates(&self) -> Result<Vec<RTCIceCandidate>> {
        read_ice!(self, |ice| ice.get_remote_candidates())
    }

    async fn get_selected_candidate_pair(&self) -> Result<Option<RTCIceCandidatePair>> {
        read_ice!(self, |ice| ice.get_selected_candidate_pair())
    }

    async fn get_local_parameters(&self) -> Result<Option<RTCIceParameters>> {
        read_ice!(self, |ice| ice.get_local_parameters())
    }

    async fn get_remote_parameters(&self) -> Result<Option<RTCIceParameters>> {
        read_ice!(self, |ice| ice.get_remote_parameters())
    }
}
