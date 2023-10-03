use async_trait::async_trait;
use util::vnet::net::*;

use super::*;
use crate::error::*;

/// `RelayAddressGeneratorNone` returns the listener with no modifications.
pub struct RelayAddressGeneratorNone {
    /// `address` is passed to Listen/ListenPacket when creating the Relay.
    pub address: String,
    pub net: Arc<Net>,
}

#[async_trait]
impl RelayAddressGenerator for RelayAddressGeneratorNone {
    fn validate(&self) -> Result<()> {
        if self.address.is_empty() {
            Err(Error::ErrListeningAddressInvalid)
        } else {
            Ok(())
        }
    }

    async fn allocate_conn(
        &self,
        use_ipv4: bool,
        requested_port: u16,
    ) -> Result<(Arc<dyn Conn + Send + Sync>, SocketAddr)> {
        let addr = self
            .net
            .resolve_addr(use_ipv4, &format!("{}:{}", self.address, requested_port))
            .await?;
        let conn = self.net.bind(addr).await?;
        let relay_addr = conn.local_addr()?;
        Ok((conn, relay_addr))
    }
}
