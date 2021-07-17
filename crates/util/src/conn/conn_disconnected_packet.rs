use super::*;

use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Since UDP is connectionless, as a server, it doesn't know how to reply
/// simply using the `Write` method. So, to make it work, `disconnectedPacketConn`
/// will infer the last packet that it reads as the reply address for `Write`
pub struct DisconnectedPacketConn {
    raddr: Mutex<SocketAddr>,
    pconn: Arc<dyn Conn + Send + Sync>,
}

impl DisconnectedPacketConn {
    pub fn new(conn: Arc<dyn Conn + Send + Sync>) -> Self {
        DisconnectedPacketConn {
            raddr: Mutex::new(SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0)),
            pconn: conn,
        }
    }
}

#[async_trait]
impl Conn for DisconnectedPacketConn {
    async fn connect(&self, addr: SocketAddr) -> Result<()> {
        self.pconn.connect(addr).await
    }

    async fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        let (n, addr) = self.pconn.recv_from(buf).await?;
        {
            let mut raddr = self.raddr.lock().await;
            *raddr = addr;
        }
        Ok(n)
    }

    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.pconn.recv_from(buf).await
    }

    async fn send(&self, buf: &[u8]) -> Result<usize> {
        let addr = {
            let raddr = self.raddr.lock().await;
            *raddr
        };
        self.pconn.send_to(buf, addr).await
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize> {
        self.pconn.send_to(buf, target).await
    }

    async fn local_addr(&self) -> Result<SocketAddr> {
        self.pconn.local_addr().await
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}
