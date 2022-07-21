use serde::{Deserialize, Serialize};

/// DataChannelParameters describes the configuration of the DataChannel.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct DataChannelParameters {
    pub label: String,
    pub protocol: String,
    pub id: u16,
    pub ordered: bool,
    pub max_packet_life_time: u16,
    pub max_retransmits: u16,
    pub negotiated: bool,
}
