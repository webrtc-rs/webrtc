// AES-CCM (Counter with CBC-MAC)
// Alternative to GCM mode.
// Available in OpenSSL as of TLS 1.3 (2018), but disabled by default.
// Two AES computations per block, thus expected to be somewhat slower than AES-GCM.
// RFC 6655 year 2012 https://tools.ietf.org/html/rfc6655
// Much lower adoption, probably because it came after GCM and offer no significant benefit.

// https://github.com/RustCrypto/AEADs
// https://docs.rs/ccm/0.3.0/ccm/ Or https://crates.io/crates/aes-ccm?

use ccm::{Ccm, consts::{U8, U12, U16}};
use ccm::aead::{Aead, NewAead, generic_array::GenericArray};
use aes::Aes256;

const CRYPTO_CCM_8_TAG_LENGTH: usize = 8;
const CRYPTO_CCM_TAG_LENGTH: usize = 16;
const CRYPTO_CCM_NONCE_LENGTH: usize = 12;

type AesCcm8 = Ccm<Aes256, U8, U12>;
type AesCcm = Ccm<Aes256, U16, U12>;

pub enum CryptoCcmTagLen {
    CryptoCcm8TagLength,
    CryptoCcmTagLength,
}

enum CryptoCcm {
    CryptoCcm8(AesCcm8),
    CryptoCcm(AesCcm),
}

// State needed to handle encrypted input/output
pub struct CryptCcm {
    local_ccm: CryptoCcm,
    remote_ccm: CryptoCcm,
    local_write_iv: Vec<u8>,
    remote_write_iv: Vec<u8>,
}

impl CryptCcm {
    pub fn new(
        tag_len: CryptoCcmTagLen,
        local_key: &[u8],
        local_write_iv: &[u8],
        remote_key: &[u8],
        remote_write_iv: &[u8],
    ) -> Self {
        let key = GenericArray::from_slice(local_key);
        let local_ccm = match tag_len {
            CryptoCcmTagLen::CryptoCcmTagLength => {
                CryptoCcm::CryptoCcm(AesCcm::new(key))
            },
            CryptoCcmTagLen::CryptoCcm8TagLength => {
                CryptoCcm::CryptoCcm8(AesCcm8::new(key))
            },
        };

        let key = GenericArray::from_slice(remote_key);
        let remote_ccm = match tag_len {
            CryptoCcmTagLen::CryptoCcmTagLength => {
                CryptoCcm::CryptoCcm(AesCcm::new(key))
            },
            CryptoCcmTagLen::CryptoCcm8TagLength => {
                CryptoCcm::CryptoCcm8(AesCcm8::new(key))
            },
        };

        CryptCcm {
            local_ccm,
            local_write_iv: local_write_iv.to_vec(),
            remote_ccm,
            remote_write_iv: remote_write_iv.to_vec(),
        }
    }
}

