use super::*;

use tokio::net::UdpSocket;

#[async_trait]
impl Conn for UdpSocket {
    // Read reads data from the connection.
    // Read can be made to time out and return an error after a fixed
    // time limit; see SetDeadline and SetReadDeadline.
    async fn recv(&self, b: &mut [u8]) -> Result<usize> {
        self.recv(b).await
    }

    // Write writes data to the connection.
    // Write can be made to time out and return an error after a fixed
    // time limit; see SetDeadline and SetWriteDeadline.
    async fn send(&self, b: &[u8]) -> Result<usize> {
        self.send(b).await
    }

    // LocalAddr returns the local network address.
    fn local_addr(&self) -> Result<SocketAddr> {
        self.local_addr()
    }
}
