pub mod dtls_role;
pub mod dtls_transport;
pub mod dtls_transport_state;

use dtls_role::*;

use crate::api::setting_engine::SettingEngine;
use crate::default_srtp_protection_profiles;
use crate::dtls::dtls_transport_state::DTLSTransportState;
use crate::error::Error;
use crate::ice::ice_role::ICERole;
use crate::ice::ice_transport::ice_transport_state::ICETransportState;
use crate::ice::ice_transport::ICETransport;
use crate::mux::endpoint::Endpoint;
use crate::mux::mux_func::{match_dtls, match_srtcp, match_srtp, MatchFunc};
use bytes::Bytes;
use dtls::config::ClientAuthType;
use dtls::conn::DTLSConn;
use dtls::crypto::Certificate;
use serde::{Deserialize, Serialize};
use srtp::protection_profile::ProtectionProfile;
use srtp::session::Session;
use srtp::stream::Stream;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use util::Conn;

/// DTLSFingerprint specifies the hash function algorithm and certificate
/// fingerprint as described in https://tools.ietf.org/html/rfc4572.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct DTLSFingerprint {
    /// Algorithm specifies one of the the hash function algorithms defined in
    /// the 'Hash function Textual Names' registry.
    pub algorithm: String,

    /// Value specifies the value of the certificate fingerprint in lowercase
    /// hex string as expressed utilizing the syntax of 'fingerprint' in
    /// https://tools.ietf.org/html/rfc4572#section-5.
    pub value: String,
}

/// DTLSParameters holds information relating to DTLS configuration.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct DTLSParameters {
    pub role: DTLSRole,
    pub fingerprints: Vec<DTLSFingerprint>,
}

pub type OnStateChangeHdlrFn = Box<
    dyn (FnMut(DTLSTransportState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;
