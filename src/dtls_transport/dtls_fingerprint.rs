use serde::{Deserialize, Serialize};

/// DTLSFingerprint specifies the hash function algorithm and certificate
/// fingerprint as described in [RFC 4572].
///
/// ## Specifications
///
/// * [W3C]
///
/// [W3C]: https://w3c.github.io/webrtc-pc/#rtcdtlsfingerprint
/// [RFC 4572]: https://tools.ietf.org/html/rfc4572
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RTCDtlsFingerprint {
    /// Algorithm specifies one of the the hash function algorithms defined in
    /// the 'Hash function Textual Names' registry.
    pub algorithm: String,

    /// Value specifies the value of the certificate fingerprint in lowercase
    /// hex string as expressed utilizing the syntax of 'fingerprint' in
    /// <https://tools.ietf.org/html/rfc4572#section-5>.
    pub value: String,
}
