//! Peer connection types and event handling
//!
//! This module provides async-friendly wrappers around the Sans-I/O rtc crate.
//!
//! # Configuration
//!
//! WebRTC peer connections require configuration for codecs, network settings, and interceptors.
//! All configuration types are re-exported from this module for convenience:
//!
//! - [`MediaEngine`] - Configure media codecs (VP8, H264, Opus, etc.)
//! - [`SettingEngine`] - Configure network timeouts, NAT settings, and security  
//! - [`interceptor_registry`] - Configure RTP/RTCP interceptors (NACK, stats, etc.)
//! - [`RTCIceServer`] - STUN/TURN server configuration
//! - [`RTCIceTransportPolicy`] - ICE transport policy (all, relay)
//! - [`RTCBundlePolicy`] - RTP bundling strategy
//! - [`RTCRtcpMuxPolicy`] - RTCP multiplexing policy
//! - [`RTCCertificate`] - Custom TLS certificates
//!
//! ## Basic Configuration with Codecs
//!
//! ```no_run
//! use std::sync::Arc;
//! use webrtc::peer_connection::{
//!     MediaEngine, PeerConnection, PeerConnectionEventHandler,
//!     RTCConfigurationBuilder, RTCIceServer,
//! };
//!
//! # #[derive(Clone)]
//! # struct MyHandler;
//! # #[async_trait::async_trait]
//! # impl PeerConnectionEventHandler for MyHandler {}
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Configure media engine with standard codecs
//! let mut media_engine = MediaEngine::default();
//! media_engine.register_default_codecs()?;
//!
//! // Configure ICE servers
//! let ice_servers = vec![RTCIceServer {
//!     urls: vec!["stun:stun.l.google.com:19302".to_string()],
//!     username: "".to_string(),
//!     credential: "".to_string(),
//! }];
//!
//! // Create peer connection configuration
//! let config = RTCConfigurationBuilder::new()
//!     .with_media_engine(media_engine)
//!     .with_ice_servers(ice_servers)
//!     .build();
//!
//! let handler = Arc::new(MyHandler);
//! let pc = PeerConnection::new(config, handler)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Advanced Configuration
//!
//! For advanced use cases, use SettingEngine for network/timeout settings:
//!
//! ```no_run
//! use webrtc::peer_connection::{
//!     MediaEngine, SettingEngine, RTCConfigurationBuilder,
//!     RTCIceServer, RTCIceTransportPolicy, RTCBundlePolicy,
//! };
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Media engine (codecs)
//! let mut media_engine = MediaEngine::default();
//! media_engine.register_default_codecs()?;
//!
//! // Setting engine (network/timeout settings)
//! let mut setting_engine = SettingEngine::default();
//! setting_engine.set_ice_timeouts(
//!     Some(std::time::Duration::from_secs(5)),  // disconnect timeout
//!     Some(std::time::Duration::from_secs(10)), // failed timeout
//!     Some(std::time::Duration::from_secs(1)),  // keepalive interval
//! );
//!
//! // ICE servers and transport policy
//! let ice_servers = vec![RTCIceServer {
//!     urls: vec!["stun:stun.l.google.com:19302".to_string()],
//!     username: "".to_string(),
//!     credential: "".to_string(),
//! }];
//!
//! let config = RTCConfigurationBuilder::new()
//!     .with_media_engine(media_engine)
//!     .with_setting_engine(setting_engine)
//!     .with_ice_servers(ice_servers)
//!     .with_ice_transport_policy(RTCIceTransportPolicy::All)
//!     .with_bundle_policy(RTCBundlePolicy::MaxBundle)
//!     .build();
//! # Ok(())
//! # }
//! ```
//!
//! For interceptor configuration (NACK, RTCP reports, etc.), see the
//! [`interceptor_registry`] module documentation in the rtc crate.
//!
//! # Re-exported Configuration Types
//!
//! - [`MediaEngine`] - Codec configuration (VP8, H264, Opus, etc.)
//! - [`SettingEngine`] - Network timeouts, NAT settings, security options
//! - [`interceptor_registry`] - RTP/RTCP interceptor chain (NACK, TWCC, reports)

mod connection;
mod driver;
mod event_handler;
mod ice_gatherer;

pub(crate) use connection::InnerMessage;
pub use connection::PeerConnection;
pub use driver::PeerConnectionDriver;
pub use event_handler::PeerConnectionEventHandler;
pub use ice_gatherer::{RTCIceGatherer, RTCIceGathererState};

// Re-export common types from rtc
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
