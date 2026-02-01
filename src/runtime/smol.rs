//! smol runtime implementation

use super::*;
use async_io::{Async, Timer};
use std::sync::Arc;

/// A WebRTC runtime for smol
#[derive(Debug)]
pub struct SmolRuntime;

impl Runtime for SmolRuntime {
    fn new_timer(&self, t: Instant) -> Pin<Box<dyn AsyncTimer>> {
        Box::pin(Timer::at(t))
    }

    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        ::smol::spawn(future).detach();
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Box<dyn AsyncUdpSocket>> {
        Ok(Box::new(UdpSocket::new(sock)?))
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

impl AsyncTimer for Timer {
    fn reset(mut self: Pin<&mut Self>, t: Instant) {
        self.set_at(t)
    }

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        Future::poll(self, cx).map(|_| ())
    }
}

#[derive(Debug, Clone)]
struct UdpSocket {
    io: Arc<Async<std::net::UdpSocket>>,
}

impl UdpSocket {
    fn new(sock: std::net::UdpSocket) -> io::Result<Self> {
        Ok(Self {
            io: Arc::new(Async::new_nonblocking(sock)?),
        })
    }
}

impl UdpSenderHelperSocket for UdpSocket {
    fn max_transmit_segments(&self) -> usize {
        1 // TODO: Support GSO if available
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        match self
            .io
            .get_ref()
            .send_to(transmit.contents, transmit.destination)
        {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Err(e),
            Err(e) => Err(e),
        }
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
        if bufs.is_empty() || meta.is_empty() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty buffers",
            )));
        }

        loop {
            match self.io.poll_readable(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }

            // Try to receive from the socket
            match self.io.get_ref().recv_from(&mut bufs[0]) {
                Ok((len, addr)) => {
                    meta[0] = RecvMeta {
                        addr,
                        len,
                        dst_addr: None,
                    };
                    return Poll::Ready(Ok(1));
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // Socket wasn't actually readable, wait again
                    continue;
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    fn may_fragment(&self) -> bool {
        false // TODO: Check platform capabilities
    }

    fn max_receive_segments(&self) -> usize {
        1 // TODO: Support GRO if available
    }
}
