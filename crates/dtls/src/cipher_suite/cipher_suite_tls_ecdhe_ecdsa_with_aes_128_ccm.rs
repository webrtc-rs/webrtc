use super::*;
use crate::cipher_suite::cipher_suite_aes_128_ccm::CipherSuiteAes128Ccm;
use crate::crypto::crypto_ccm::CryptoCcmTagLen;

pub fn new_cipher_suite_tls_ecdhe_ecdsa_with_aes_128_ccm() -> CipherSuiteAes128Ccm {
    CipherSuiteAes128Ccm::new(
        ClientCertificateType::ECDSASign,
        CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_CCM,
        false,
        CryptoCcmTagLen::CryptoCcmTagLength,
    )
}
