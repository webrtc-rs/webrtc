//! Runtime-agnostic networking utilities

use std::io;
use std::net::SocketAddr;

/// Runtime-agnostic DNS resolution
pub async fn resolve_host(host: &str) -> io::Result<Vec<SocketAddr>> {
    #[cfg(feature = "runtime-tokio")]
    {
        tokio::net::lookup_host(host)
            .await
            .map(|iter| iter.collect())
    }

    #[cfg(feature = "runtime-smol")]
    {
        ::smol::net::resolve(host).await
    }
}

/// Runtime-agnostic UDP socket for one-shot operations (like STUN queries)
pub struct UdpSocket {
    #[cfg(feature = "runtime-tokio")]
    inner: tokio::net::UdpSocket,
    #[cfg(feature = "runtime-smol")]
    inner: ::smol::net::UdpSocket,
}

impl UdpSocket {
    pub async fn bind(addr: &str) -> io::Result<Self> {
        #[cfg(feature = "runtime-tokio")]
        {
            Ok(Self {
                inner: tokio::net::UdpSocket::bind(addr).await?,
            })
        }

        #[cfg(feature = "runtime-smol")]
        {
            Ok(Self {
                inner: ::smol::net::UdpSocket::bind(addr).await?,
            })
        }
    }

    pub async fn send_to(&self, buf: &[u8], addr: SocketAddr) -> io::Result<usize> {
        self.inner.send_to(buf, addr).await
    }

    pub async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}
