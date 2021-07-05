pub mod sctp_transport;
pub mod sctp_transport_state;

use serde::{Deserialize, Serialize};

/// SCTPCapabilities indicates the capabilities of the SCTPTransport.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct SCTPCapabilities {
    max_message_size: u32,
}
