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
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
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

pub(crate) const SDP_ATTRIBUTE_RID: &str = "rid";
pub(crate) const GENERATED_CERTIFICATE_ORIGIN: &str = "WebRTC";
pub(crate) const SDES_REPAIR_RTP_STREAM_ID_URI: &str =
    "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmallStr(SmolStr);

impl Serialize for SmallStr {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.as_str())
    }
}

impl<'de> Deserialize<'de> for SmallStr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(SmallStr(SmolStr::new(s)))
    }
}
