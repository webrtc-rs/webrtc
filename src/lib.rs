#![warn(rust_2018_idioms)]
#![allow(dead_code)]

// re-export sub-crates
pub use data;
pub use dtls;
pub use ice;
pub use interceptor;
pub use mdns;
pub use media;
pub use rtcp;
pub use rtp;
pub use sctp;
pub use sdp;
pub use srtp;
pub use stun;
pub use turn;
pub use util;

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
