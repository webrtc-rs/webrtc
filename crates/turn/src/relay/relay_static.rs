use super::*;
use crate::errors::*;

use std::net::IpAddr;
use tokio::net::UdpSocket;

use async_trait::async_trait;

// RelayAddressGeneratorStatic can be used to return static IP address each time a relay is created.
// This can be used when you have a single static IP address that you want to use
pub struct RelayAddressGeneratorStatic {
    // RelayAddress is the IP returned to the user when the relay is created
    pub relay_address: IpAddr,

    // Address is passed to Listen/ListenPacket when creating the Relay
    pub address: String,
}

#[async_trait]
impl RelayAddressGenerator for RelayAddressGeneratorStatic {
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
        let mut relay_addr = conn.local_addr()?;
        relay_addr.set_ip(self.relay_address);
        return Ok((Arc::new(conn), relay_addr));
    }
}
