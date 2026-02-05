//! Tokio runtime implementation

use super::*;
use std::sync::Arc;

/// A WebRTC runtime for Tokio
#[derive(Debug)]
pub struct TokioRuntime;

impl Runtime for TokioRuntime {
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

#[derive(Debug, Clone)]
struct UdpSocket {
    io: Arc<::tokio::net::UdpSocket>,
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


/// Runtime-agnostic sleep function
#[cfg(feature = "runtime-tokio")]
pub async fn sleep(duration: Duration) {
    ::tokio::time::sleep(duration).await
}

/// Runtime-agnostic timeout helper
///
/// Returns Ok(result) if the future completes within the duration,
/// or Err(()) if the timeout expires.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, ()>
where
    F: std::future::Future<Output = T>,
{
    ::tokio::time::timeout(duration, future)
        .await
        .map_err(|_| ())
}
