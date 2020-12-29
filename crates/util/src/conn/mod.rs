pub mod conn_pipe;
pub mod conn_tcp;
pub mod conn_udp;

#[cfg(test)]
mod conn_pipe_test;

use async_trait::async_trait;
use std::io::Result;
use std::net::SocketAddr;

#[async_trait]
pub trait Conn {
    // Read reads data from the connection.
    // Read can be made to time out and return an error after a fixed
    // time limit; see SetDeadline and SetReadDeadline.
    async fn recv(&self, b: &mut [u8]) -> Result<usize>;

    // Write writes data to the connection.
    // Write can be made to time out and return an error after a fixed
    // time limit; see SetDeadline and SetWriteDeadline.
    async fn send(&self, b: &[u8]) -> Result<usize>;

    // LocalAddr returns the local network address.
    fn local_addr(&self) -> Result<SocketAddr>;

    // RemoteAddr returns the remote network address.
    // fn remote_addr(&self) -> Result<SocketAddr>;
}
