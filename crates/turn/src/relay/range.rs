use super::*;
use crate::errors::*;

use std::net::IpAddr;
use tokio::net::UdpSocket;

use async_trait::async_trait;

// RelayAddressGeneratorRanges can be used to only allocate connections inside a defined port range
pub struct RelayAddressGeneratorRanges {
    // relay_address is the IP returned to the user when the relay is created
    pub relay_address: IpAddr,

    // min_port the minimum port to allocate
    pub min_port: u16,

    // max_port the maximum (inclusive) port to allocate
    pub max_port: u16,

    // max_retries the amount of tries to allocate a random port in the defined range
    pub max_retries: u16,

    // Address is passed to Listen/ListenPacket when creating the Relay
    pub address: String,
}

#[async_trait]
impl RelayAddressGenerator for RelayAddressGeneratorRanges {
    // validate confirms that the RelayAddressGenerator is properly initialized
    fn validate(&self) -> Result<(), Error> {
        if self.min_port == 0 {
            Err(ERR_MIN_PORT_NOT_ZERO.to_owned())
        } else if self.max_port == 0 {
            Err(ERR_MAX_PORT_NOT_ZERO.to_owned())
        } else if self.max_port < self.min_port {
            Err(ERR_MAX_PORT_LESS_THAN_MIN_PORT.to_owned())
        } else if self.address.is_empty() {
            Err(ERR_LISTENING_ADDRESS_INVALID.to_owned())
        } else {
            Ok(())
        }
    }

    // Allocate a PacketConn (UDP) relay_address
    async fn allocate_conn(
        &self,
        _network: &str,
        requested_port: u16,
    ) -> Result<(Arc<dyn Conn + Send + Sync>, SocketAddr), Error> {
        let max_retries = if self.max_retries == 0 {
            10
        } else {
            self.max_retries
        };

        if requested_port != 0 {
            let conn = UdpSocket::bind(format!("{}:{}", self.address, requested_port)).await?;
            let mut relay_addr = conn.local_addr()?;
            relay_addr.set_ip(self.relay_address);
            return Ok((Arc::new(conn), relay_addr));
        }

        for _ in 0..max_retries {
            let port = self.min_port + rand::random::<u16>() % (self.max_port + 1 - self.min_port);
            let conn = match UdpSocket::bind(format!("{}:{}", self.address, port)).await {
                Ok(conn) => conn,
                Err(_) => continue,
            };

            let mut relay_addr = conn.local_addr()?;
            relay_addr.set_ip(self.relay_address);
            return Ok((Arc::new(conn), relay_addr));
        }

        Err(ERR_MAX_RETRIES_EXCEEDED.to_owned())
    }
}
