#![warn(rust_2018_idioms)]
#![allow(dead_code)]

pub use {data, dtls, ice, interceptor, mdns, media, rtcp, rtp, sctp, sdp, srtp, stun, turn, util};

/// [`peer_connection::RTCPeerConnection`] allows to establish connection between two peers given RTC configuration. Its API is similar to one in JavaScript.
pub mod peer_connection;

/// The utilities defining transport between peers. Contains [`ice_transport::ice_server::RTCIceServer`] struct which describes how peer does ICE (Interactive Connectivity Establishment).
pub mod ice_transport;

/// WebRTC DataChannel can be used for peer-to-peer transmitting arbitrary binary data.
pub mod data_channel;

/// Module responsible for multiplexing data streams of different protocols on one socket. Custom [`mux::endpoint::Endpoint`] with [`mux::mux_func::MatchFunc`] can be used for parsing your application-specific byte stream.
pub mod mux; // TODO: why is this public? does someone really extend WebRTC stack?

/// Measuring connection statistics, such as amount of data transmitted or round trip time.
pub mod stats;

/// [`Error`] enumerates WebRTC problems, [`error::OnErrorHdlrFn`] defines type for callback-logger.
pub mod error;

/// Set of constructors for WebRTC primitives. Subject to deprecation in future.
pub mod api;

pub mod dtls_transport;
pub mod rtp_transceiver;
pub mod sctp_transport;
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
