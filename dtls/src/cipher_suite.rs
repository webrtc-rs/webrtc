use std::fmt;

use super::client_certificate_type::*;
use super::record_layer::*;

use util::Error;

// CipherSuiteID is an ID for our supported CipherSuites
// Supported Cipher Suites
#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq)]
pub enum CipherSuiteID {
    // AES-128-CCM
    TLS_ECDHE_ECDSA_WITH_AES_128_CCM = 0xc0ac,
    TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8 = 0xc0ae,

    // AES-128-GCM-SHA256
    TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 = 0xc02b,
    TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 = 0xc02f,

    // AES-256-CBC-SHA
    TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA = 0xc00a,
    TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA = 0xc014,

    TLS_PSK_WITH_AES_128_CCM = 0xc0a4,
    TLS_PSK_WITH_AES_128_CCM_8 = 0xc0a8,
    TLS_PSK_WITH_AES_128_GCM_SHA256 = 0x00a8,

    Unsupported,
}

impl fmt::Display for CipherSuiteID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM => {
                write!(f, "TLS_ECDHE_ECDSA_WITH_AES_128_CCM")
            }
            CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8 => {
                write!(f, "TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8")
            }
            CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 => {
                write!(f, "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256")
            }
            CipherSuiteID::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 => {
                write!(f, "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256")
            }
            CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA => {
                write!(f, "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA")
            }
            CipherSuiteID::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA => {
                write!(f, "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA")
            }
            CipherSuiteID::TLS_PSK_WITH_AES_128_CCM => write!(f, "TLS_PSK_WITH_AES_128_CCM"),
            CipherSuiteID::TLS_PSK_WITH_AES_128_CCM_8 => write!(f, "TLS_PSK_WITH_AES_128_CCM_8"),
            CipherSuiteID::TLS_PSK_WITH_AES_128_GCM_SHA256 => {
                write!(f, "TLS_PSK_WITH_AES_128_GCM_SHA256")
            }
            _ => write!(f, "Unsupported CipherSuiteID"),
        }
    }
}

pub trait CipherSuite {
    fn to_string(&self) -> String;
    fn id(&self) -> CipherSuiteID;
    fn certificate_type(&self) -> ClientCertificateType;
    //TODO: fn hash_func() -> func() hash.Hash;
    fn is_psk(&self) -> bool;
    fn is_initialized(&self) -> bool;

    // Generate the internal encryption state
    fn init(
        &mut self,
        master_secret: &[u8],
        client_random: &[u8],
        server_random: &[u8],
        is_client: bool,
    ) -> Result<(), Error>;

    fn encrypt(&self, pkt: &RecordLayer, raw: &[u8]) -> Result<Vec<u8>, Error>;
    fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>, Error>;
}
