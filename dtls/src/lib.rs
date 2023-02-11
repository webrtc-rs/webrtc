#![warn(rust_2018_idioms)]
#![allow(dead_code)]

pub mod alert;
pub mod application_data;
pub mod change_cipher_spec;
pub mod cipher_suite;
pub mod client_certificate_type;
pub mod compression_methods;
pub mod config;
pub mod conn;
pub mod content;
pub mod crypto;
pub mod curve;
mod error;
pub mod extension;
pub mod flight;
pub mod fragment_buffer;
pub mod handshake;
pub mod handshaker;
pub mod listener;
pub mod prf;
pub mod record_layer;
pub mod signature_hash_algorithm;
pub mod state;

use cipher_suite::*;
pub use error::Error;
use extension::extension_use_srtp::SrtpProtectionProfile;

pub(crate) fn find_matching_srtp_profile(
    a: &[SrtpProtectionProfile],
    b: &[SrtpProtectionProfile],
) -> Result<SrtpProtectionProfile, ()> {
    for a_profile in a {
        for b_profile in b {
            if a_profile == b_profile {
                return Ok(*a_profile);
            }
        }
    }
    Err(())
}

pub(crate) fn find_matching_cipher_suite(
    a: &[CipherSuiteId],
    b: &[CipherSuiteId],
) -> Result<CipherSuiteId, ()> {
    for a_suite in a {
        for b_suite in b {
            if a_suite == b_suite {
                return Ok(*a_suite);
            }
        }
    }
    Err(())
}
