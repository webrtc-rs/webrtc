pub mod dtls_role;
pub mod dtls_transport_state;

use dtls_role::*;

use serde::{Deserialize, Serialize};

/// DTLSFingerprint specifies the hash function algorithm and certificate
/// fingerprint as described in https://tools.ietf.org/html/rfc4572.
#[derive(Default, Debug, Serialize, Deserialize)]
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
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct DTLSParameters {
    pub role: DTLSRole,
    pub fingerprints: Vec<DTLSFingerprint>,
}
