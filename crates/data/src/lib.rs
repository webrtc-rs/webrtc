mod channel_type;
pub mod data_channel;
pub mod exact_size_buf;
pub mod marshal;
pub mod message;

// This basically a stub for the still incomplete 'webrtc-sctp' crate:
// https://crates.io/crates/webrtc-sctp
mod sctp;

pub use channel_type::ChannelType;