#![warn(rust_2018_idioms)]
#![allow(dead_code)]

//! # WebRTC Crate Overview
//! The `webrtc` crate provides Rust-based bindings and high-level abstractions
//! for WebRTC, based on the [W3C specification](https://www.w3.org/TR/webrtc/).
//! Included is a set of communication protocols and APIs for building real-time
//! communication (RTC) applications on top of the WebRTC standard.
//!
//! If you would like to learn more about WebRTC in general, the
//! [WebRTC for the Curious](https://webrtcforthecurious.com/) book is a free
//! resource that provides a great introduction to the topic.
//!
//! ## Features
//! - Connections to remote peers using NAT-traversal technologies (STUN, TURN, and ICE)
//! - Streaming of audio and video media via RTP and RTCP
//! - Data channels for high performance, bi-directional communication
//! - Secured communications via DTLS and SRTP
//! - Multi-homing and congestion control using SCTP
//! - Support for Multicast DNS (mDNS)
//! - Interceptors for RTP, RTCP, and DataChannel packets
//!
//! ## Key Concepts
//!
//! The WebRTC API, as defined by the W3C specification, is composed of a number of
//! constructs and interfaces that provide a rich set of functionality, including
//! (but not limited to):
//!  - connection establishment
//!  - media streaming
//!  - data transfer
//!  - error handling
//!  - congestion control
//!
//! The following section provides a brief overview of the key concepts and constructs
//! that are used throughout the WebRTC API.
//!
//! ### RTCConfiguration
//!
//! The [`RTCConfiguration`] struct defines the set of parameters that are used to configure
//! how peer-to-peer communication via [`RTCPeerConnection`] is established or re-established.
//! This includes the set of ICE servers to use, the ICE transport policy, the bundle policy,
//! the RTCP mux policy, the peer identity, and the set of certificates to use.
//!
//! Configurations may be reused across multiple connections, and are treated as read-only
//! once constructed.
//!
//! ### RTCPeerConnection
//!
//! The [`RTCPeerConnection`] is the primary entry point to the WebRTC API. It represents an
//! individual connection between a local device and a remote peer.
//!
//! ### MediaStream
//!
//! ### DataChannel
//!
//! ### RTCIceCandidate
//!
//! ### RTCSessionDescription
//!
//! ## Examples
//! The `examples/` directory contains a range of examples, from basic peer connections to
//! advanced data channel usage.
//!
//! ## Compatibility
//! This crate aims to stay up-to-date with the latest W3C WebRTC specification. However,
//! as WebRTC is a rapidly evolving standard, there might be minor discrepancies. Always
//! refer to the official W3C WebRTC specification for authoritative information.
//!
//! ## License
//! This project is licensed under either of the following, at your option:
//! - [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
//! - [MIT License](https://opensource.org/license/mit/)
//!
//! [`RTCConfiguration`]: crate::peer_connection::configuration::RTCConfiguration
//! [`RTCPeerConnection`]: crate::peer_connection::RTCPeerConnection

// re-export sub-crates
pub use {data, dtls, ice, interceptor, mdns, media, rtcp, rtp, sctp, sdp, srtp, stun, turn, util};

pub mod api;
pub mod data_channel;
pub mod dtls_transport;
pub mod error;
pub mod ice_transport;
pub mod mux;
pub mod peer_connection;
pub mod rtp_transceiver;
pub mod sctp_transport;
pub mod stats;
pub mod track;

pub use error::Error;

#[macro_use]
extern crate lazy_static;

pub(crate) const UNSPECIFIED_STR: &str = "Unspecified";

/// Equal to UDP MTU
pub(crate) const RECEIVE_MTU: usize = 1460;

pub(crate) const SDP_ATTRIBUTE_RID: &str = "rid";
pub(crate) const GENERATED_CERTIFICATE_ORIGIN: &str = "WebRTC";
pub(crate) const SDES_REPAIR_RTP_STREAM_ID_URI: &str =
    "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id";
