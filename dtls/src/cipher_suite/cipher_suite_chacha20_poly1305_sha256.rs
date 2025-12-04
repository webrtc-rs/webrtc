use super::*;
use crate::crypto::crypto_chacha20::*;
use crate::prf::*;

#[derive(Clone)]
pub struct CipherSuiteChaCha20Poly1305Sha256 {
    rsa: bool,
    cipher: Option<CryptoChaCha20>,
}

impl CipherSuiteChaCha20Poly1305Sha256 {
    const PRF_MAC_LEN: usize = 0;
    const PRF_KEY_LEN: usize = 32;
    const PRF_IV_LEN: usize = 12;

    pub fn new(rsa: bool) -> Self {
        CipherSuiteChaCha20Poly1305Sha256 { rsa, cipher: None }
    }
}

impl CipherSuite for CipherSuiteChaCha20Poly1305Sha256 {
    fn to_string(&self) -> String {
        if self.rsa {
            "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256".to_owned()
        } else {
            "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256".to_owned()
        }
    }

    fn id(&self) -> CipherSuiteId {
        if self.rsa {
            CipherSuiteId::Tls_Ecdhe_Rsa_With_ChaCha20_Poly1305_Sha256
        } else {
            CipherSuiteId::Tls_Ecdhe_Ecdsa_With_ChaCha20_Poly1305_Sha256
        }
    }

    fn certificate_type(&self) -> ClientCertificateType {
        if self.rsa {
            ClientCertificateType::RsaSign
        } else {
            ClientCertificateType::EcdsaSign
        }
    }

    fn hash_func(&self) -> CipherSuiteHash {
        CipherSuiteHash::Sha256
    }

    fn is_psk(&self) -> bool {
        false
    }

    fn is_initialized(&self) -> bool {
        self.cipher.is_some()
    }

    fn init(
        &mut self,
        master_secret: &[u8],
        client_random: &[u8],
        server_random: &[u8],
        is_client: bool,
    ) -> Result<()> {
        let keys = prf_encryption_keys(
            master_secret,
            client_random,
            server_random,
            CipherSuiteChaCha20Poly1305Sha256::PRF_MAC_LEN,
            CipherSuiteChaCha20Poly1305Sha256::PRF_KEY_LEN,
            CipherSuiteChaCha20Poly1305Sha256::PRF_IV_LEN,
            self.hash_func(),
        )?;

        if is_client {
            self.cipher = Some(CryptoChaCha20::new(
                &keys.client_write_key,
                &keys.client_write_iv,
                &keys.server_write_key,
                &keys.server_write_iv,
            ));
        } else {
            self.cipher = Some(CryptoChaCha20::new(
                &keys.server_write_key,
                &keys.server_write_iv,
                &keys.client_write_key,
                &keys.client_write_iv,
            ));
        }

        Ok(())
    }

    fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>> {
        let cg = self.cipher.as_ref().ok_or(Error::Other(
            "CipherSuite has not been initialized, unable to encrypt".to_owned(),
        ))?;
        cg.encrypt(pkt_rlh, raw)
    }

    fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>> {
        let cg = self.cipher.as_ref().ok_or(Error::Other(
            "CipherSuite has not been initialized, unable to decrypt".to_owned(),
        ))?;
        cg.decrypt(input)
    }
}
