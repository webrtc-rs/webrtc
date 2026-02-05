//! smol runtime implementation

use super::*;
use ::smol::net::UdpSocket as SmolUdpSocket;
use ::smol::spawn;
use std::sync::Arc;

/// A WebRTC runtime for smol
#[derive(Debug)]
pub struct SmolRuntime;

impl Runtime for SmolRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        spawn(future).detach();
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Box<dyn AsyncUdpSocket>> {
        Ok(Box::new(UdpSocket::new(sock)?))
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[derive(Debug)]
struct UdpSocket {
    io: Arc<SmolUdpSocket>,
}

impl UdpSocket {
    fn new(sock: std::net::UdpSocket) -> io::Result<Self> {
        // Wrap std socket in smol's Async
        let async_sock = ::smol::Async::new(sock)?;
        Ok(Self {
            io: Arc::new(SmolUdpSocket::from(async_sock)),
        })
    }
}

impl AsyncUdpSocket for UdpSocket {
    fn send_to<'a>(
        &'a self,
        buf: &'a [u8],
        target: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        Box::pin(async move { self.io.send_to(buf, target).await })
    }

    fn recv_from<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<(usize, SocketAddr)>> + Send + 'a>> {
        Box::pin(async move { self.io.recv_from(buf).await })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }
}

pub async fn sleep(duration: Duration) {
    ::smol::Timer::after(duration).await;
}

/// Runtime-agnostic timeout helper
///
/// Returns Ok(result) if the future completes within the duration,
/// or Err(()) if the timeout expires.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, ()>
where
    F: std::future::Future<Output = T>,
{
    ::smol::future::or(async { Ok(future.await) }, async {
        sleep(duration).await;
        Err(())
    })
    .await
}
