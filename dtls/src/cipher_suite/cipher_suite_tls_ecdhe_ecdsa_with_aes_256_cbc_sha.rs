use super::*;
use crate::crypto::crypto_cbc::*;
use crate::prf::*;

use std::sync::Arc;
use tokio::sync::Mutex;

use async_trait::async_trait;

pub struct CipherSuiteTLSEcdheEcdsaWithAes256CbcSha {
    cbc: Arc<Mutex<Option<CryptoCbc>>>,
}

impl CipherSuiteTLSEcdheEcdsaWithAes256CbcSha {
    const PRF_MAC_LEN: usize = 20;
    const PRF_KEY_LEN: usize = 32;
    const PRF_IV_LEN: usize = 16;

    pub fn new() -> Self {
        CipherSuiteTLSEcdheEcdsaWithAes256CbcSha {
            cbc: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl CipherSuite for CipherSuiteTLSEcdheEcdsaWithAes256CbcSha {
    fn to_string(&self) -> String {
        "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA".to_owned()
    }

    fn id(&self) -> CipherSuiteID {
        CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
    }

    fn certificate_type(&self) -> ClientCertificateType {
        ClientCertificateType::ECDSASign
    }

    fn hash_func(&self) -> CipherSuiteHash {
        CipherSuiteHash::SHA256
    }

    fn is_psk(&self) -> bool {
        false
    }

    async fn is_initialized(&self) -> bool {
        let cbc = self.cbc.lock().await;
        cbc.is_some()
    }

    async fn init(
        &mut self,
        master_secret: &[u8],
        client_random: &[u8],
        server_random: &[u8],
        is_client: bool,
    ) -> Result<(), Error> {
        let keys = prf_encryption_keys(
            master_secret,
            client_random,
            server_random,
            CipherSuiteTLSEcdheEcdsaWithAes256CbcSha::PRF_MAC_LEN,
            CipherSuiteTLSEcdheEcdsaWithAes256CbcSha::PRF_KEY_LEN,
            CipherSuiteTLSEcdheEcdsaWithAes256CbcSha::PRF_IV_LEN,
            self.hash_func(),
        )?;

        let mut cbc = self.cbc.lock().await;
        if is_client {
            *cbc = Some(CryptoCbc::new(
                &keys.client_write_key,
                &keys.client_write_iv,
                &keys.client_mac_key,
                &keys.server_write_key,
                &keys.server_write_iv,
                &keys.server_mac_key,
            )?);
        } else {
            *cbc = Some(CryptoCbc::new(
                &keys.server_write_key,
                &keys.server_write_iv,
                &keys.server_mac_key,
                &keys.client_write_key,
                &keys.client_write_iv,
                &keys.client_mac_key,
            )?);
        }

        Ok(())
    }

    async fn encrypt(&self, pkt: &RecordLayer, raw: &[u8]) -> Result<Vec<u8>, Error> {
        let mut cbc = self.cbc.lock().await;
        if let Some(cg) = cbc.as_mut() {
            cg.encrypt(pkt, raw)
        } else {
            Err(Error::new(
                "CipherSuite has not been initialized, unable to encrypt".to_owned(),
            ))
        }
    }

    async fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        let mut cbc = self.cbc.lock().await;
        if let Some(cg) = cbc.as_mut() {
            cg.decrypt(input)
        } else {
            Err(Error::new(
                "CipherSuite has not been initialized, unable to decrypt".to_owned(),
            ))
        }
    }
}
