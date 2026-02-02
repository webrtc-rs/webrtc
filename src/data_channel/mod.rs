//! Data channel module for async WebRTC
//!
//! This module provides async-friendly wrappers around the Sans-I/O rtc data channel.

mod channel;

pub use channel::DataChannel;

// Re-export common types from rtc
pub use rtc::data_channel::{
    RTCDataChannelId, RTCDataChannelInit, RTCDataChannelMessage, RTCDataChannelState,
};
