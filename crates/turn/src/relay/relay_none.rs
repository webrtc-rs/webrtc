use super::*;
use crate::errors::*;

use tokio::net::UdpSocket;

use async_trait::async_trait;

// RelayAddressGeneratorNone returns the listener with no modifications
pub struct RelayAddressGeneratorNone {
    // Address is passed to Listen/ListenPacket when creating the Relay
    pub address: String,
}

#[async_trait]
impl RelayAddressGenerator for RelayAddressGeneratorNone {
    // validate confirms that the RelayAddressGenerator is properly initialized
    fn validate(&self) -> Result<(), Error> {
        if self.address.is_empty() {
            Err(ERR_LISTENING_ADDRESS_INVALID.to_owned())
        } else {
            Ok(())
        }
    }

    // Allocate a PacketConn (UDP) RelayAddress
    async fn allocate_conn(
        &self,
        _network: &str,
        requested_port: u16,
    ) -> Result<(Arc<dyn Conn + Send + Sync>, SocketAddr), Error> {
        let conn = UdpSocket::bind(format!("{}:{}", self.address, requested_port)).await?;
        let relay_addr = conn.local_addr()?;
        Ok((Arc::new(conn), relay_addr))
    }
}
