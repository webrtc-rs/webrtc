use serde::{Deserialize, Serialize};

/// SCTPTransportCapabilities indicates the capabilities of the SCTPTransport.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub struct SCTPTransportCapabilities {
    pub max_message_size: u32,
}
