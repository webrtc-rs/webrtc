#![warn(rust_2018_idioms)]
#![allow(dead_code)]

//! Async-friendly WebRTC implementation in Rust
//!
//! This crate provides a runtime-agnostic WebRTC implementation built on top of
//! the Sans-I/O [rtc](https://docs.rs/rtc) protocol core.
//!
//! # Runtime Support
//!
//! The library supports multiple async runtimes through feature flags:
//!
//! - `runtime-tokio` (default) - Tokio runtime support
//! - `runtime-smol` - smol runtime support

pub mod data_channel;
pub(crate) mod ice_gatherer;
pub mod media_track;
pub mod peer_connection;
pub(crate) mod peer_connection_driver;
pub mod rtp_transceiver;
pub mod runtime;

// Re-export common types from rtc
pub use rtc::data_channel::{
    RTCDataChannelId, RTCDataChannelInit, RTCDataChannelMessage, RTCDataChannelState,
};
pub use rtc::interceptor::Registry;
pub use rtc::media_stream::{MediaStreamId, MediaStreamTrack, MediaStreamTrackId};
pub use rtc::peer_connection::{
    RTCPeerConnection,
    certificate::RTCCertificate,
    configuration::{
        RTCBundlePolicy, RTCConfiguration, RTCConfigurationBuilder, RTCIceServer,
        RTCIceTransportPolicy, RTCRtcpMuxPolicy, interceptor_registry::*,
        media_engine::MediaEngine, setting_engine::SettingEngine,
    },
    event::{
        RTCDataChannelEvent, RTCPeerConnectionEvent, RTCPeerConnectionIceErrorEvent,
        RTCPeerConnectionIceEvent, RTCTrackEvent,
    },
    sdp::{RTCSdpType, RTCSessionDescription},
    state::{
        RTCIceConnectionState, RTCIceGatheringState, RTCPeerConnectionState, RTCSignalingState,
    },
    transport::{RTCIceCandidate, RTCIceCandidateInit, RTCIceCandidateType, RTCIceProtocol},
};
pub use rtc::rtp_transceiver::{
    RTCRtpReceiverId, RTCRtpSenderId, RTCRtpTransceiverDirection, RTCRtpTransceiverInit,
};
pub use rtc::shared::error::{Error, Result};
