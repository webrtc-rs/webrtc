pub mod conn_bridge;
pub mod conn_pipe;
pub mod conn_udp;

#[cfg(test)]
mod conn_bridge_test;
#[cfg(test)]
mod conn_pipe_test;
#[cfg(test)]
mod conn_test;

use async_trait::async_trait;
use std::io::Result;
use std::net::SocketAddr;
use tokio::net::ToSocketAddrs;

#[async_trait]
pub trait Conn {
    async fn connect(&self, addr: SocketAddr) -> Result<()>;
    async fn recv(&self, buf: &mut [u8]) -> Result<usize>;
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)>;
    async fn send(&self, buf: &[u8]) -> Result<usize>;
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize>;
    async fn local_addr(&self) -> Result<SocketAddr>;
}

pub async fn lookup_host<T>(use_ipv4: bool, host: T) -> Result<SocketAddr>
where
    T: ToSocketAddrs,
{
    for remote_addr in tokio::net::lookup_host(host).await? {
        if (use_ipv4 && remote_addr.is_ipv4()) || (!use_ipv4 && remote_addr.is_ipv6()) {
            return Ok(remote_addr);
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!(
            "No available {} IP address found!",
            if use_ipv4 { "ipv4" } else { "ipv6" },
        ),
    ))
}
