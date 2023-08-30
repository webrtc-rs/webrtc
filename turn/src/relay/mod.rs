pub mod relay_none;
pub mod relay_range;
pub mod relay_static;

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use util::Conn;

use crate::error::Result;

/// `RelayAddressGenerator` is used to generate a Relay Address when creating an allocation.
/// You can use one of the provided ones or provide your own.
#[async_trait]
pub trait RelayAddressGenerator {
    /// Confirms that this is properly initialized
    fn validate(&self) -> Result<()>;

    /// Allocates a Relay Address
    async fn allocate_conn(
        &self,
        use_ipv4: bool,
        requested_port: u16,
    ) -> Result<(Arc<dyn Conn + Send + Sync>, SocketAddr)>;
}
