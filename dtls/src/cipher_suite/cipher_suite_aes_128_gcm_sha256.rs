use super::*;
use crate::crypto::crypto_gcm::*;
use crate::prf::*;

#[derive(Clone)]
pub struct CipherSuiteAes128GcmSha256 {
    gcm: Option<CryptoGcm>,
    rsa: bool,
}

impl CipherSuiteAes128GcmSha256 {
    const PRF_MAC_LEN: usize = 0;
    const PRF_KEY_LEN: usize = 16;
    const PRF_IV_LEN: usize = 4;

    pub fn new(rsa: bool) -> Self {
        CipherSuiteAes128GcmSha256 { gcm: None, rsa }
    }
}

impl CipherSuite for CipherSuiteAes128GcmSha256 {
    fn to_string(&self) -> String {
        if self.rsa {
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256".to_owned()
        } else {
            "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256".to_owned()
        }
    }

    fn id(&self) -> CipherSuiteId {
        if self.rsa {
            CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256
        } else {
            CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256
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
        self.gcm.is_some()
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
            CipherSuiteAes128GcmSha256::PRF_MAC_LEN,
            CipherSuiteAes128GcmSha256::PRF_KEY_LEN,
            CipherSuiteAes128GcmSha256::PRF_IV_LEN,
            self.hash_func(),
        )?;

        if is_client {
            self.gcm = Some(CryptoGcm::new(
                &keys.client_write_key,
                &keys.client_write_iv,
                &keys.server_write_key,
                &keys.server_write_iv,
            ));
        } else {
            self.gcm = Some(CryptoGcm::new(
                &keys.server_write_key,
                &keys.server_write_iv,
                &keys.client_write_key,
                &keys.client_write_iv,
            ));
        }

        Ok(())
    }

    fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>> {
        if let Some(cg) = &self.gcm {
            cg.encrypt(pkt_rlh, raw)
        } else {
            Err(Error::Other(
                "CipherSuite has not been initialized, unable to encrypt".to_owned(),
            ))
        }
    }

    fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>> {
        if let Some(cg) = &self.gcm {
            cg.decrypt(input)
        } else {
            Err(Error::Other(
                "CipherSuite has not been initialized, unable to decrypt".to_owned(),
            ))
        }
    }
}
