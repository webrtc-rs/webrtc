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

use cipher_suite_aes_128_gcm_sha256::*;
use cipher_suite_aes_256_cbc_sha::*;
use cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm::*;
use cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm8::*;
use cipher_suite_tls_psk_with_aes_128_ccm::*;
use cipher_suite_tls_psk_with_aes_128_ccm8::*;
use cipher_suite_tls_psk_with_aes_128_gcm_sha256::*;

use super::client_certificate_type::*;
use super::error::*;
use super::record_layer::record_layer_header::*;

// CipherSuiteID is an ID for our supported CipherSuites
// Supported Cipher Suites
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CipherSuiteId {
    // AES-128-CCM
    Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm = 0xc0ac,
    Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm_8 = 0xc0ae,

    // AES-128-GCM-SHA256
    Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256 = 0xc02b,
    Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256 = 0xc02f,

    // AES-256-CBC-SHA
    Tls_Ecdhe_Ecdsa_With_Aes_256_Cbc_Sha = 0xc00a,
    Tls_Ecdhe_Rsa_With_Aes_256_Cbc_Sha = 0xc014,

    Tls_Psk_With_Aes_128_Ccm = 0xc0a4,
    Tls_Psk_With_Aes_128_Ccm_8 = 0xc0a8,
    Tls_Psk_With_Aes_128_Gcm_Sha256 = 0x00a8,

    Unsupported,
}

impl fmt::Display for CipherSuiteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm => {
                write!(f, "TLS_ECDHE_ECDSA_WITH_AES_128_CCM")
            }
            CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm_8 => {
                write!(f, "TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8")
            }
            CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256 => {
                write!(f, "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256")
            }
            CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256 => {
                write!(f, "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256")
            }
            CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_256_Cbc_Sha => {
                write!(f, "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA")
            }
            CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_256_Cbc_Sha => {
                write!(f, "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA")
            }
            CipherSuiteId::Tls_Psk_With_Aes_128_Ccm => write!(f, "TLS_PSK_WITH_AES_128_CCM"),
            CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8 => write!(f, "TLS_PSK_WITH_AES_128_CCM_8"),
            CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256 => {
                write!(f, "TLS_PSK_WITH_AES_128_GCM_SHA256")
            }
            _ => write!(f, "Unsupported CipherSuiteID"),
        }
    }
}

impl From<u16> for CipherSuiteId {
    fn from(val: u16) -> Self {
        match val {
            // AES-128-CCM
            0xc0ac => CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm,
            0xc0ae => CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm_8,

            // AES-128-GCM-SHA256
            0xc02b => CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256,
            0xc02f => CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256,

            // AES-256-CBC-SHA
            0xc00a => CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_256_Cbc_Sha,
            0xc014 => CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_256_Cbc_Sha,

            0xc0a4 => CipherSuiteId::Tls_Psk_With_Aes_128_Ccm,
            0xc0a8 => CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8,
            0x00a8 => CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256,

            _ => CipherSuiteId::Unsupported,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum CipherSuiteHash {
    Sha256,
}

impl CipherSuiteHash {
    pub(crate) fn size(&self) -> usize {
        match *self {
            CipherSuiteHash::Sha256 => 32,
        }
    }
}

pub trait CipherSuite {
    fn to_string(&self) -> String;
    fn id(&self) -> CipherSuiteId;
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
    ) -> Result<()>;

    fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>>;
    fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>>;
}

// Taken from https://www.iana.org/assignments/tls-parameters/tls-parameters.xml
// A cipher_suite is a specific combination of key agreement, cipher and MAC
// function.
pub fn cipher_suite_for_id(id: CipherSuiteId) -> Result<Box<dyn CipherSuite + Send + Sync>> {
    match id {
        CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm => {
            Ok(Box::new(new_cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm()))
        }
        CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Ccm_8 => Ok(Box::new(
            new_cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm8(),
        )),
        CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256 => {
            Ok(Box::new(CipherSuiteAes128GcmSha256::new(false)))
        }
        CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256 => {
            Ok(Box::new(CipherSuiteAes128GcmSha256::new(true)))
        }
        CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_256_Cbc_Sha => {
            Ok(Box::new(CipherSuiteAes256CbcSha::new(true)))
        }
        CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_256_Cbc_Sha => {
            Ok(Box::new(CipherSuiteAes256CbcSha::new(false)))
        }
        CipherSuiteId::Tls_Psk_With_Aes_128_Ccm => {
            Ok(Box::new(new_cipher_suite_tls_psk_with_aes_128_ccm()))
        }
        CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8 => {
            Ok(Box::new(new_cipher_suite_tls_psk_with_aes_128_ccm8()))
        }
        CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256 => {
            Ok(Box::<CipherSuiteTlsPskWithAes128GcmSha256>::default())
        }
        _ => Err(Error::ErrInvalidCipherSuite),
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
        Box::<CipherSuiteTlsPskWithAes128GcmSha256>::default(),
    ]
}

fn cipher_suites_for_ids(ids: &[CipherSuiteId]) -> Result<Vec<Box<dyn CipherSuite + Send + Sync>>> {
    let mut cipher_suites = vec![];
    for id in ids {
        cipher_suites.push(cipher_suite_for_id(*id)?);
    }
    Ok(cipher_suites)
}

pub(crate) fn parse_cipher_suites(
    user_selected_suites: &[CipherSuiteId],
    exclude_psk: bool,
    exclude_non_psk: bool,
) -> Result<Vec<Box<dyn CipherSuite + Send + Sync>>> {
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
        Err(Error::ErrNoAvailableCipherSuites)
    } else {
        Ok(filtered_cipher_suites)
    }
}
