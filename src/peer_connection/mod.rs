//! Peer connection types and event handling
//!
//! This module provides async-friendly wrappers around the Sans-I/O rtc crate.

mod connection;
mod driver;
mod event_handler;

pub use connection::PeerConnection;
pub use driver::PeerConnectionDriver;
pub use event_handler::PeerConnectionEventHandler;

// Re-export common types from rtc
pub use rtc::peer_connection::{
    RTCPeerConnection as RTCPeerConnectionCore,
    configuration::{
        media_engine::MediaEngine, RTCConfiguration, RTCConfigurationBuilder,
    },
    event::{
        RTCDataChannelEvent, RTCPeerConnectionEvent, RTCPeerConnectionIceErrorEvent,
        RTCPeerConnectionIceEvent, RTCTrackEvent,
    },
    sdp::{RTCSdpType, RTCSessionDescription},
    state::{
        RTCIceConnectionState, RTCIceGatheringState, RTCPeerConnectionState, RTCSignalingState,
    },
};
