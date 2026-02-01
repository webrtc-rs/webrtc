//! Tokio runtime implementation

use super::*;
use std::sync::Arc;
use std::task::ready;

/// A WebRTC runtime for Tokio
#[derive(Debug)]
pub struct TokioRuntime;

impl Runtime for TokioRuntime {
    fn new_timer(&self, deadline: Instant) -> Pin<Box<dyn AsyncTimer>> {
        Box::pin(::tokio::time::sleep_until(deadline.into()))
    }

    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        ::tokio::spawn(future);
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Box<dyn AsyncUdpSocket>> {
        Ok(Box::new(UdpSocket {
            io: Arc::new(::tokio::net::UdpSocket::from_std(sock)?),
        }))
    }

    fn now(&self) -> Instant {
        ::tokio::time::Instant::now().into_std()
    }
}

impl AsyncTimer for ::tokio::time::Sleep {
    fn reset(self: Pin<&mut Self>, deadline: Instant) {
        Self::reset(self, deadline.into())
    }

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        Future::poll(self, cx)
    }
}

#[derive(Debug, Clone)]
struct UdpSocket {
    io: Arc<::tokio::net::UdpSocket>,
}

impl UdpSenderHelperSocket for UdpSocket {
    fn max_transmit_segments(&self) -> usize {
        1 // TODO: Support GSO if available
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        self.io.try_io(::tokio::io::Interest::WRITABLE, || {
            self.io
                .try_send_to(transmit.contents, transmit.destination)
                .map(|_| ())
        })
    }
}

impl AsyncUdpSocket for UdpSocket {
    fn create_sender(&self) -> Pin<Box<dyn UdpSender>> {
        Box::pin(UdpSenderHelper::new(self.clone(), |socket: &Self| {
            let socket = socket.clone();
            async move { socket.io.writable().await }
        }))
    }

    fn poll_recv(
        &mut self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        loop {
            ready!(self.io.poll_recv_ready(cx))?;

            // Try to receive from the socket
            if let Ok(res) = self.io.try_io(::tokio::io::Interest::READABLE, || {
                // Read into the first buffer
                if bufs.is_empty() || meta.is_empty() {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty buffers"));
                }

                match self.io.try_recv_from(&mut bufs[0]) {
                    Ok((len, addr)) => {
                        meta[0] = RecvMeta {
                            addr,
                            len,
                            dst_addr: None,
                        };
                        Ok(1)
                    }
                    Err(e) => Err(e),
                }
            }) {
                return Poll::Ready(Ok(res));
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }

    fn may_fragment(&self) -> bool {
        false // TODO: Check platform capabilities
    }

    fn max_receive_segments(&self) -> usize {
        1 // TODO: Support GRO if available
    }
}
