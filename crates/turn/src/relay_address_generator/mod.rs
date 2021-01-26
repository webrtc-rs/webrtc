pub mod relay_address_generator_none;
pub mod relay_address_generator_range;
pub mod relay_address_generator_static;

use util::{Conn, Error};

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

use async_trait::async_trait;

// RelayAddressGenerator is used to generate a RelayAddress when creating an allocation.
// You can use one of the provided ones or provide your own.
#[async_trait]
pub(crate) trait RelayAddressGenerator {
    // validate confirms that the RelayAddressGenerator is properly initialized
    fn validate(&self) -> Result<(), Error>;

    // Allocate a RelayAddress
    async fn allocate_conn(
        &self,
        network: &str,
        requested_port: u16,
    ) -> Result<(Arc<dyn Conn + Send + Sync>, SocketAddr), Error>;
}
