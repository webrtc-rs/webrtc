use std::net::IpAddr;

use async_trait::async_trait;
use util::vnet::net::*;

use super::*;
use crate::error::*;

/// `RelayAddressGeneratorRanges` can be used to only allocate connections inside a defined port range.
pub struct RelayAddressGeneratorRanges {
    /// `relay_address` is the IP returned to the user when the relay is created.
    pub relay_address: IpAddr,

    /// `min_port` the minimum port to allocate.
    pub min_port: u16,

    /// `max_port` the maximum (inclusive) port to allocate.
    pub max_port: u16,

    /// `max_retries` the amount of tries to allocate a random port in the defined range.
    pub max_retries: u16,

    /// `address` is passed to Listen/ListenPacket when creating the Relay.
    pub address: String,

    pub net: Arc<Net>,
}

#[async_trait]
impl RelayAddressGenerator for RelayAddressGeneratorRanges {
    fn validate(&self) -> Result<()> {
        if self.min_port == 0 {
            Err(Error::ErrMinPortNotZero)
        } else if self.max_port == 0 {
            Err(Error::ErrMaxPortNotZero)
        } else if self.max_port < self.min_port {
            Err(Error::ErrMaxPortLessThanMinPort)
        } else if self.address.is_empty() {
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
        let max_retries = if self.max_retries == 0 {
            10
        } else {
            self.max_retries
        };

        if requested_port != 0 {
            let addr = self
                .net
                .resolve_addr(use_ipv4, &format!("{}:{}", self.address, requested_port))
                .await?;
            let conn = self.net.bind(addr).await?;
            let mut relay_addr = conn.local_addr()?;
            relay_addr.set_ip(self.relay_address);
            return Ok((conn, relay_addr));
        }

        for _ in 0..max_retries {
            let port = self.min_port + rand::random::<u16>() % (self.max_port - self.min_port + 1);
            let addr = self
                .net
                .resolve_addr(use_ipv4, &format!("{}:{}", self.address, port))
                .await?;
            let conn = match self.net.bind(addr).await {
                Ok(conn) => conn,
                Err(_) => continue,
            };

            let mut relay_addr = conn.local_addr()?;
            relay_addr.set_ip(self.relay_address);
            return Ok((conn, relay_addr));
        }

        Err(Error::ErrMaxRetriesExceeded)
    }
}
