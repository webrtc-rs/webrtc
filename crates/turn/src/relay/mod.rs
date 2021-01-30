pub mod none;
pub mod range;
pub mod r#static;

use util::{Conn, Error};

use std::net::SocketAddr;
use std::sync::Arc;

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
