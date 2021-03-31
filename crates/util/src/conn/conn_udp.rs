use super::*;

use tokio::net::UdpSocket;

#[async_trait]
impl Conn for UdpSocket {
    async fn connect(&self, addr: SocketAddr) -> Result<()> {
        self.connect(addr).await
    }

    async fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        self.recv(buf).await
    }

    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.recv_from(buf).await
    }

    async fn send(&self, buf: &[u8]) -> Result<usize> {
        self.send(buf).await
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize> {
        self.send_to(buf, target).await
    }

    async fn local_addr(&self) -> Result<SocketAddr> {
        self.local_addr()
    }
}
