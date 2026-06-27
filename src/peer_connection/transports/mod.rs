use rtc::shared::FourTuple;
use std::io;
use std::net::SocketAddr;

pub(crate) mod stun_gatherer;
pub(crate) mod tcp_transport;
pub(crate) mod turn_relayer;

pub(crate) enum SocketRecvResult {
    Packet {
        n: usize,
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
