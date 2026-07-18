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
