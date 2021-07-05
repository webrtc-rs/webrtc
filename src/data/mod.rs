pub mod data_channel;
pub mod data_channel_state;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// DataChannelConfig can be used to configure properties of the underlying
/// channel such as data reliability.
#[derive(Default, Debug, Clone)]
pub struct DataChannelConfig {
    /// ordered indicates if data is allowed to be delivered out of order. The
    /// default value of true, guarantees that data will be delivered in order.
    pub ordered: bool,

    /// max_packet_life_time limits the time (in milliseconds) during which the
    /// channel will transmit or retransmit data if not acknowledged. This value
    /// may be clamped if it exceeds the maximum value supported.
    pub max_packet_life_time: u16,

    /// max_retransmits limits the number of times a channel will retransmit data
    /// if not successfully delivered. This value may be clamped if it exceeds
    /// the maximum value supported.
    pub max_retransmits: u16,

    /// protocol describes the subprotocol name used for this channel.
    pub protocol: String,

    /// negotiated describes if the data channel is created by the local peer or
    /// the remote peer. The default value of false tells the user agent to
    /// announce the channel in-band and instruct the other peer to dispatch a
    /// corresponding DataChannel. If set to true, it is up to the application
    /// to negotiate the channel and create an DataChannel with the same id
    /// at the other peer.
    pub negotiated: bool,

    /// id overrides the default selection of ID for this channel.
    pub id: u16,
}

/// DataChannelMessage represents a message received from the
/// data channel. IsString will be set to true if the incoming
/// message is of the string type. Otherwise the message is of
/// a binary type.
#[derive(Default, Debug, Clone)]
pub struct DataChannelMessage {
    pub is_string: bool,
    pub data: Bytes,
}

/// DataChannelParameters describes the configuration of the DataChannel.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct DataChannelParameters {
    pub label: String,
    pub protocol: String,
    pub id: u16,
    pub ordered: bool,
    pub max_packet_lifetime: u16,
    pub max_retransmits: u16,
    pub negotiated: bool,
}
