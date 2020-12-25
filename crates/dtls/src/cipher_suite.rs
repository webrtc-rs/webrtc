pub mod cipher_suite_aes_128_ccm;
pub mod cipher_suite_aes_128_gcm_sha256;
pub mod cipher_suite_aes_256_cbc_sha;
pub mod cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm;
pub mod cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm8;
pub mod cipher_suite_tls_psk_with_aes_128_ccm;
pub mod cipher_suite_tls_psk_with_aes_128_ccm8;
pub mod cipher_suite_tls_psk_with_aes_128_gcm_sha256;

use std::fmt;
use std::marker::{Send, Sync};

use super::client_certificate_type::*;
use super::errors::*;
use super::record_layer::record_layer_header::*;

use util::Error;

use cipher_suite_aes_128_gcm_sha256::*;
use cipher_suite_aes_256_cbc_sha::*;
use cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm::*;
use cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm8::*;
use cipher_suite_tls_psk_with_aes_128_ccm::*;
use cipher_suite_tls_psk_with_aes_128_ccm8::*;
use cipher_suite_tls_psk_with_aes_128_gcm_sha256::*;

// CipherSuiteID is an ID for our supported CipherSuites
// Supported Cipher Suites
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, PartialEq)]
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

impl From<u16> for CipherSuiteID {
    fn from(val: u16) -> Self {
        match val {
            // AES-128-CCM
            0xc0ac => CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM,
            0xc0ae => CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8,

            // AES-128-GCM-SHA256
            0xc02b => CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
            0xc02f => CipherSuiteID::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,

            // AES-256-CBC-SHA
            0xc00a => CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA,
            0xc014 => CipherSuiteID::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA,

            0xc0a4 => CipherSuiteID::TLS_PSK_WITH_AES_128_CCM,
            0xc0a8 => CipherSuiteID::TLS_PSK_WITH_AES_128_CCM_8,
            0x00a8 => CipherSuiteID::TLS_PSK_WITH_AES_128_GCM_SHA256,

            _ => CipherSuiteID::Unsupported,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum CipherSuiteHash {
    SHA256,
}

impl CipherSuiteHash {
    pub(crate) fn size(&self) -> usize {
        match *self {
            CipherSuiteHash::SHA256 => 32,
        }
    }
}

pub trait CipherSuite {
    fn to_string(&self) -> String;
    fn id(&self) -> CipherSuiteID;
    fn certificate_type(&self) -> ClientCertificateType;
    fn hash_func(&self) -> CipherSuiteHash;
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

    fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>, Error>;
    fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>, Error>;
}

// Taken from https://www.iana.org/assignments/tls-parameters/tls-parameters.xml
// A cipher_suite is a specific combination of key agreement, cipher and MAC
// function.
pub fn cipher_suite_for_id(id: CipherSuiteID) -> Result<Box<dyn CipherSuite + Send + Sync>, Error> {
    match id {
        CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM => {
            Ok(Box::new(new_cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm()))
        }
        CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8 => Ok(Box::new(
            new_cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm8(),
        )),
        CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 => {
            Ok(Box::new(CipherSuiteAes128GcmSha256::new(false)))
        }
        CipherSuiteID::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 => {
            Ok(Box::new(CipherSuiteAes128GcmSha256::new(true)))
        }
        CipherSuiteID::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA => {
            Ok(Box::new(CipherSuiteAes256CbcSha::new(true)))
        }
        CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA => {
            Ok(Box::new(CipherSuiteAes256CbcSha::new(false)))
        }
        CipherSuiteID::TLS_PSK_WITH_AES_128_CCM => {
            Ok(Box::new(new_cipher_suite_tls_psk_with_aes_128_ccm()))
        }
        CipherSuiteID::TLS_PSK_WITH_AES_128_CCM_8 => {
            Ok(Box::new(new_cipher_suite_tls_psk_with_aes_128_ccm8()))
        }
        CipherSuiteID::TLS_PSK_WITH_AES_128_GCM_SHA256 => {
            Ok(Box::new(CipherSuiteTLSPskWithAes128GcmSha256::default()))
        }
        _ => Err(ERR_INVALID_CIPHER_SUITE.clone()),
    }
}

// CipherSuites we support in order of preference
pub(crate) fn default_cipher_suites() -> Vec<Box<dyn CipherSuite + Send + Sync>> {
    vec![
        Box::new(CipherSuiteAes128GcmSha256::new(false)),
        Box::new(CipherSuiteAes256CbcSha::new(false)),
        Box::new(CipherSuiteAes128GcmSha256::new(true)),
        Box::new(CipherSuiteAes256CbcSha::new(true)),
    ]
}

fn all_cipher_suites() -> Vec<Box<dyn CipherSuite + Send + Sync>> {
    vec![
        Box::new(new_cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm()),
        Box::new(new_cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm8()),
        Box::new(CipherSuiteAes128GcmSha256::new(false)),
        Box::new(CipherSuiteAes128GcmSha256::new(true)),
        Box::new(CipherSuiteAes256CbcSha::new(false)),
        Box::new(CipherSuiteAes256CbcSha::new(true)),
        Box::new(new_cipher_suite_tls_psk_with_aes_128_ccm()),
        Box::new(new_cipher_suite_tls_psk_with_aes_128_ccm8()),
        Box::new(CipherSuiteTLSPskWithAes128GcmSha256::default()),
    ]
}

fn cipher_suites_for_ids(
    ids: &[CipherSuiteID],
) -> Result<Vec<Box<dyn CipherSuite + Send + Sync>>, Error> {
    let mut cipher_suites = vec![];
    for id in ids {
        cipher_suites.push(cipher_suite_for_id(*id)?);
    }
    Ok(cipher_suites)
}

pub(crate) fn parse_cipher_suites(
    user_selected_suites: &[CipherSuiteID],
    exclude_psk: bool,
    exclude_non_psk: bool,
) -> Result<Vec<Box<dyn CipherSuite + Send + Sync>>, Error> {
    let cipher_suites = if !user_selected_suites.is_empty() {
        cipher_suites_for_ids(user_selected_suites)?
    } else {
        default_cipher_suites()
    };

    let filtered_cipher_suites: Vec<Box<dyn CipherSuite + Send + Sync>> = cipher_suites
        .into_iter()
        .filter(|c| !((exclude_psk && c.is_psk()) || (exclude_non_psk && !c.is_psk())))
        .collect();

    if filtered_cipher_suites.is_empty() {
        Err(ERR_NO_AVAILABLE_CIPHER_SUITES.clone())
    } else {
        Ok(filtered_cipher_suites)
    }
}
