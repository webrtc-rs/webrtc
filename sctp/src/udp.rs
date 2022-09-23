use std::{
    io::{self, IoSliceMut},
    net::{IpAddr, Ipv6Addr, SocketAddr},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use futures_util::ready;
use log::warn;
use proto::{EcnCodepoint, Payload, Transmit};
use tokio::io::ReadBuf;

#[derive(Debug, Copy, Clone)]
pub struct RecvMeta {
    pub addr: SocketAddr,
    pub len: usize,
    pub ecn: Option<EcnCodepoint>,
    /// The destination IP address which was encoded in this datagram
    pub dst_ip: Option<IpAddr>,
}

impl Default for RecvMeta {
    /// Constructs a value with arbitrary fields, intended to be overwritten
    fn default() -> Self {
        Self {
            addr: SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0),
            len: 0,
            ecn: None,
            dst_ip: None,
        }
    }
}

/// Log at most 1 IO error per minute
const IO_ERROR_LOG_INTERVAL: Duration = std::time::Duration::from_secs(60);

/// Logs a warning message when sendmsg fails
///
/// Logging will only be performed if at least [`IO_ERROR_LOG_INTERVAL`]
/// has elapsed since the last error was logged.
fn log_sendmsg_error(
    last_send_error: &mut Instant,
    err: impl core::fmt::Debug,
    transmit: &Transmit,
) {
    let now = Instant::now();
    if now.saturating_duration_since(*last_send_error) > IO_ERROR_LOG_INTERVAL {
        *last_send_error = now;
        warn!(
        "sendmsg error: {:?}, Transmit: {{ reote: {:?}, local_ip: {:?}, enc: {:?}, payload: {:?}, }}",
            err, transmit.remote, transmit.local_ip, transmit.ecn, transmit.payload);
    }
}

/// Tokio-compatible UDP socket with some useful specializations.
///
/// Unlike a standard tokio UDP socket, this allows ECN bits to be read and written on some
/// platforms.
#[derive(Debug)]
pub struct UdpSocket {
    io: tokio::net::UdpSocket,
    last_send_error: Instant,
}

impl UdpSocket {
    pub fn from_std(socket: std::net::UdpSocket) -> io::Result<UdpSocket> {
        socket.set_nonblocking(true)?;
        let now = Instant::now();
        Ok(UdpSocket {
            io: tokio::net::UdpSocket::from_std(socket)?,
            last_send_error: now.checked_sub(2 * IO_ERROR_LOG_INTERVAL).unwrap_or(now),
        })
    }

    pub fn poll_send(
        &mut self,
        cx: &mut Context<'_>,
        transmits: &[Transmit],
    ) -> Poll<Result<usize, io::Error>> {
        let mut sent = 0;
        for transmit in transmits {
            if let Payload::RawEncode(contents) = &transmit.payload {
                for content in contents {
                    match self.io.poll_send_to(cx, content, transmit.remote) {
                        Poll::Ready(Ok(_)) => {
                            sent += 1;
                        }
                        // We need to report that some packets were sent in this case, so we rely on
                        // errors being either harmlessly transient (in the case of WouldBlock) or
                        // recurring on the next call.
                        Poll::Ready(Err(_)) | Poll::Pending if sent != 0 => {
                            return Poll::Ready(Ok(sent))
                        }
                        Poll::Ready(Err(e)) => {
                            // WouldBlock is expected to be returned as `Poll::Pending`
                            debug_assert!(e.kind() != io::ErrorKind::WouldBlock);

                            // Errors are ignored, since they will ususally be handled
                            // by higher level retransmits and timeouts.
                            // - PermissionDenied errors have been observed due to iptable rules.
                            //   Those are not fatal errors, since the
                            //   configuration can be dynamically changed.
                            // - Destination unreachable errors have been observed for other
                            log_sendmsg_error(&mut self.last_send_error, e, transmit);
                            sent += 1;
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                }
            }
        }
        Poll::Ready(Ok(sent))
    }

    pub fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        debug_assert!(!bufs.is_empty());
        let mut buf = ReadBuf::new(&mut bufs[0]);
        let addr = ready!(self.io.poll_recv_from(cx, &mut buf))?;
        meta[0] = RecvMeta {
            len: buf.filled().len(),
            addr,
            ecn: None,
            dst_ip: None,
        };
        Poll::Ready(Ok(1))
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }
}

pub const BATCH_SIZE: usize = 1;
