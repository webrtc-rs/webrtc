#![warn(rust_2018_idioms)]
#![allow(dead_code)]

//! # WebRTC Crate Overview
//!
//! The `webrtc` crate provides Rust-based bindings and high-level abstractions
//! for WebRTC, based on the [W3C specification](https://www.w3.org/TR/webrtc/).
//! Included is a set of communication protocols and APIs for building real-time
//! communication (RTC) applications on top of the WebRTC standard.
//!
//! If you would like to learn more about WebRTC in general, the
//! [WebRTC for the Curious](https://webrtcforthecurious.com/) book is a free
//! resource that provides a great introduction to the topic.
//!
//! # Features
//!
//! - Connections to remote peers using NAT-traversal technologies (STUN, TURN, and ICE)
//! - Streaming of audio and video media via RTP and RTCP
//! - Data channels for high performance, bi-directional communication
//! - Secured communications via DTLS and SRTP
//! - Multi-homing and congestion control using SCTP
//! - Support for Multicast DNS (mDNS)
//! - Interceptors for RTP, RTCP, and DataChannel packets
//!
//! # Key Concepts
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
//! ### Configuration
//!
//! The [`RTCConfiguration`] struct defines the set of parameters that are used to configure
//! how peer-to-peer communication via [`RTCPeerConnection`] is established or re-established.
//! This includes the set of ICE servers to use, the ICE transport policy, the bundle policy,
//! the RTCP mux policy, the peer identity, and the set of certificates to use.
//!
//! Configurations may be reused across multiple [`RTCPeerConnection`]s, and are treated as read-only
//! once constructed.
//!
//! ### Peer Connections
//!
//! The [`RTCPeerConnection`] is the primary entry point to the WebRTC API. It represents an
//! individual connection between a local device and a remote peer.
//!
//! #### State Machine
//!
//! Each [`RTCPeerConnection`] tracks four distinct states as part of its state machine:
//!
//! | State Machine | Getter Method | Event Handler Method | Enum |
//! | ------------- | ------------- | -------------------- | ---- |
//! | Signaling state | [`signaling_state()`](crate::peer_connection::RTCPeerConnection::signaling_state) | [`on_signaling_state_change()`](crate::peer_connection::RTCPeerConnection::on_signaling_state_change) | [`RTCSignalingState`](crate::peer_connection::signaling_state::RTCSignalingState) |
//! | ICE connection state | [`ice_connection_state()`](crate::peer_connection::RTCPeerConnection::ice_connection_state) | [`on_ice_connection_state_change()`](crate::peer_connection::RTCPeerConnection::on_ice_connection_state_change) | [`RTCIceConnectionState`](crate::ice_transport::ice_connection_state::RTCIceConnectionState) |
//! | ICE gathering state | [`ice_gathering_state()`](crate::peer_connection::RTCPeerConnection::ice_gathering_state) | [`on_ice_gathering_state_change()`](crate::peer_connection::RTCPeerConnection::on_ice_gathering_state_change) | [`RTCIceGatheringState`](crate::ice_transport::ice_gathering_state::RTCIceGatheringState) |
//! | Peer connection state | [`connection_state()`](crate::peer_connection::RTCPeerConnection::connection_state) !! | [`on_peer_connection_state_change()`](crate::peer_connection::RTCPeerConnection::on_peer_connection_state_change) | [`RTCPeerConnectionState`](crate::peer_connection::peer_connection_state::RTCPeerConnectionState) |
//!
//! You can define event handlers for each of these states using the corresponding `on_*` methods,
//! passing a FnMut closure that accepts the corresponding enum type and returns a
//! `Pin<Box<dyn Future<Output = ()> + Send + 'static>` future to be awaited.
//!
//! ### Event Handling
//!
//! For clarity, the event handler methods run synchronously and accept a (synchronous) closure
//! that returns a future. Any async work that you need to do as part of an event handler should
//! be placed in the future that is returned by the closure, as the returned future will be
//! immediately awaited.
//!
//! In fact, all of the event handler methods within this crate are structured in this way.
//! While it may feel odd to be forced into returning a future from a synchronous method,
//! it allows for a mix of synchronous and asynchronous work to be done within the handler,
//! depending on your specific use case.
//!
//! **This will be a common source of confusion for new users of the crate.**
//!
//! ### Session Descriptions
//!
//! In the WebRTC protocol, session descriptions serve as the mechanism for exchanging
//! information about media capabilities, network addresses, and other metadata between
//! peers. Session descriptions are represented by the [`RTCSessionDescription`] struct.
//!
//! Session descriptions are exchanged via an offer/answer model, where one peer sends
//! an offer to the other peer, and the other peer responds with an answer. Offers and
//! answers are represented by the [`RTCOfferOptions`] and [`RTCAnswerOptions`] structs,
//! respectively.
//!
//! On the wire, session descriptions are encoded as SDP
//! ([Session Description Protocol](https://en.wikipedia.org/wiki/Session_Description_Protocol))
//! documents.
//!
//! ### Signaling
//!
//! In order to establish a connection, both peers must exchange their session descriptions
//! with each other. The process of exchanging of session descriptions between peers is
//! more commonly referred to as the signaling process.
//!
//! At a high level, the signaling process looks something like this:
//!
//! | Step # | Peer | Action | Method |
//! | :----: | :--: |--------|--------|
//! | 1 | Peer A | Creates an offer | [`create_offer()`](crate::peer_connection::RTCPeerConnection::create_offer) |
//! | 2 | Peer A | Sets the offer as the local description | [`set_local_description()`](crate::peer_connection::RTCPeerConnection::set_local_description) |
//! | 3 | Peer A | Sends the offer to Peer B via the signaling channel | |
//! | 4 | Peer B | Receives the offer from the signaling channel | |
//! | 5 | Peer B | Sets the received offer as the remote description | [`set_remote_description()`](crate::peer_connection::RTCPeerConnection::set_remote_description) |
//! | 6 | Peer B | Creates an answer | [`create_answer()`](crate::peer_connection::RTCPeerConnection::create_answer) |
//! | 7 | Peer B | Sets the answer as the local description | [`set_local_description()`](crate::peer_connection::RTCPeerConnection::set_local_description) |
//! | 8 | Peer B | Sends the answer to Peer A via the signaling channel | |
//! | 9 | Peer A | Receives the answer from the signaling channel | |
//! | 10 | Peer A | Sets the received answer as the remote description | [`set_remote_description()`](crate::peer_connection::RTCPeerConnection::set_remote_description) |
//! | 11 | Both | Are now connected | |
//!
//! #### No Automatic Signaling in WebRTC
//!
//! **In the WebRTC protocol, the signaling process does not happen automatically.**
//!
//! Signaling is outside the scope of the WebRTC specification, and is left up to the
//! application to implement. In other words, you will have to provide your own signaling
//! implementation. There is generally no one-size-fits-all solution for signaling, as
//! it is highly dependent on the specific use case (which may need to consider things
//! such as user authentication, security, encryption, etc.).
//!
//! Common signaling methods include (but may not be limited to):
//! - WebSockets
//! - HTTPS (e.g. using a REST API)
//! - SIP (Session Initiation Protocol)
//! - XMPP (Extensible Messaging and Presence Protocol)
//!
//! As signaling is an application-specific concern, this crate does not provide any
//! built-in signaling functionality or guidance on how to implement.
//!
//! ### ICE Agent
//!
//! The [`Agent`](ice::agent::Agent) struct implements the ICE ([Interactive Connectivity Establishment](https://en.wikipedia.org/wiki/Interactive_Connectivity_Establishment))
//! protocol, which is used to gather local ICE candidates, as well as manage the state of the
//! ICE transport for a given peer connection.
//!
//! ICE agent's configuration parameters are defined by the [`RTCConfiguration`] struct.
//!
//! Certain [`RTCPeerConnection`] methods interact with the ICE agent, including:
//!
//! - [`add_ice_candidate()`](crate::peer_connection::RTCPeerConnection::add_ice_candidate)
//! - [`set_local_description()`](crate::peer_connection::RTCPeerConnection::set_local_description)
//! - [`set_remote_description()`](crate::peer_connection::RTCPeerConnection::set_remote_description)
//! - [`close()`](crate::peer_connection::RTCPeerConnection::close)
//!
//! These interactions are described in [RFC8829](https://tools.ietf.org/html/rfc8829). The ICE
//! agency also provides indications when the state of an ICE transport changes via the event
//! handler methods that are available within [`RTCPeerConnection`].
//!
//! ### MediaStream
//!
//! ### DataChannel
//!
//! ### RTCIceCandidate
//!
//! ### RTCSessionDescription
//!
//! # Examples
//!
//! The `examples/` directory contains a range of examples, from basic peer connections to
//! advanced data channel usage.
//!
//! # Compatibility
//!
//! This crate aims to stay up-to-date with the latest W3C WebRTC specification. However,
//! as WebRTC is a rapidly evolving standard, there might be minor discrepancies. Always
//! refer to the official W3C WebRTC specification for authoritative information.
//!
//! # License
//!
//! This project is licensed under either of the following, at your option:
//! - [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
//! - [MIT License](https://opensource.org/license/mit/)
//!
//! [`RTCConfiguration`]: crate::peer_connection::configuration::RTCConfiguration
//! [`RTCPeerConnection`]: crate::peer_connection::RTCPeerConnection
//! [`RTCSessionDescription`]: crate::peer_connection::sdp::session_description::RTCSessionDescription
//! [`RTCOfferOptions`]: crate::peer_connection::offer_answer_options::RTCOfferOptions
//! [`RTCAnswerOptions`]: crate::peer_connection::offer_answer_options::RTCAnswerOptions

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
pub(crate) const SDP_ATTRIBUTE_SIMULCAST: &str = "simulcast";
pub(crate) const GENERATED_CERTIFICATE_ORIGIN: &str = "WebRTC";
pub(crate) const SDES_REPAIR_RTP_STREAM_ID_URI: &str =
    "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id";
