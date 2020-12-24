mod relay_address_generator_none;
mod relay_address_generator_range;
mod relay_address_generator_static;

use util::Error;

use std::net::SocketAddr;
use tokio::net::{TcpSocket, UdpSocket};

use async_trait::async_trait;

// RelayAddressGenerator is used to generate a RelayAddress when creating an allocation.
// You can use one of the provided ones or provide your own.
#[async_trait]
pub(crate) trait RelayAddressGenerator {
    // validate confirms that the RelayAddressGenerator is properly initialized
    fn validate(&self) -> Result<(), Error>;

    // Allocate a PacketConn (UDP) RelayAddress
    async fn allocate_packet_conn(
        &self,
        network: &str,
        requested_port: u16,
    ) -> Result<(UdpSocket, SocketAddr), Error>;

    // Allocate a Conn (TCP) RelayAddress
    async fn allocate_conn(
        &self,
        network: &str,
        requested_port: u16,
    ) -> Result<(TcpSocket, SocketAddr), Error>;
}
