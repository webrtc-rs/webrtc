pub mod conn_pipe;
pub mod conn_udp;

#[cfg(test)]
mod conn_pipe_test;

use async_trait::async_trait;
use std::io::Result;
use std::net::SocketAddr;

#[async_trait]
pub trait Conn {
    async fn connect(&self, addr: SocketAddr) -> Result<()>;
    async fn recv(&self, buf: &mut [u8]) -> Result<usize>;
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)>;
    async fn send(&self, buf: &[u8]) -> Result<usize>;
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize>;
    fn local_addr(&self) -> Result<SocketAddr>;
}
