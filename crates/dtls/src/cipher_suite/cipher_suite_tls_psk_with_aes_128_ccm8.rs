use super::*;
use crate::cipher_suite::cipher_suite_aes_128_ccm::CipherSuiteAes128Ccm;
use crate::crypto::crypto_ccm::CryptoCcmTagLen;

pub fn new_cipher_suite_tls_psk_with_aes_128_ccm8() -> CipherSuiteAes128Ccm {
    CipherSuiteAes128Ccm::new(
        ClientCertificateType::Unsupported,
        CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8,
        true,
        CryptoCcmTagLen::CryptoCcm8TagLength,
    )
}
