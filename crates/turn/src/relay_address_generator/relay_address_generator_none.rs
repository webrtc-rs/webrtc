use super::*;
use crate::errors::*;

use async_trait::async_trait;

// RelayAddressGeneratorNone returns the listener with no modifications
pub struct RelayAddressGeneratorNone {
    // Address is passed to Listen/ListenPacket when creating the Relay
    pub address: String,
    //pub socket: UdpSocket, //Net *vnet.Net
}

#[async_trait]
impl RelayAddressGenerator for RelayAddressGeneratorNone {
    // validate confirms that the RelayAddressGenerator is properly initialized
    fn validate(&self) -> Result<(), Error> {
        if self.address.is_empty() {
            Err(ERR_LISTENING_ADDRESS_INVALID.clone())
        } else {
            Ok(())
        }
    }

    // Allocate a PacketConn (UDP) RelayAddress
    async fn allocate_packet_conn(
        &self,
        _network: &str,
        requested_port: u16,
    ) -> Result<(UdpSocket, SocketAddr), Error> {
        let conn = UdpSocket::bind(format!("{}:{}", self.address, requested_port)).await?;
        let local_addr = conn.local_addr()?;
        Ok((conn, local_addr))
    }

    // Allocate a Conn (TCP) RelayAddress
    async fn allocate_conn(
        &self,
        _network: &str,
        _requested_port: u16,
    ) -> Result<(TcpSocket, SocketAddr), Error> {
        Err(ERR_TODO.clone())
    }
}
