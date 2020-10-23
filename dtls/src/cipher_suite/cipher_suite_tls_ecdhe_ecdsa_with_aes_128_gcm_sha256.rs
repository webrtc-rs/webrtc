use super::*;
use crate::crypto::crypto_gcm::*;

use std::sync::Arc;
use tokio::sync::Mutex;

use async_trait::async_trait;

const PRF_MAC_LEN: usize = 0;
const PRF_KEY_LEN: usize = 16;
const PRF_IV_LEN: usize = 4;

pub struct CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256 {
    gcm: Arc<Mutex<Option<CryptoGcm>>>,
}

#[async_trait]
impl CipherSuite for CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256 {
    fn to_string(&self) -> String {
        "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256".to_owned()
    }

    fn id(&self) -> CipherSuiteID {
        CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
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
        let gcm = self.gcm.lock().await;
        gcm.is_some()
    }

    async fn init(
        &mut self,
        _master_secret: &[u8],
        _client_random: &[u8],
        _server_random: &[u8],
        _is_client: bool,
    ) -> Result<(), Error> {
        /*
        keys, err := prfEncryptionKeys(masterSecret, clientRandom, serverRandom, PRF_MAC_LEN, PRF_KEY_LEN, PRF_IV_LEN, c.hashFunc())
        if err != nil {
            return err
        }

        var gcm *CryptoGcm
        if isClient {
            gcm, err = newCryptoGCM(keys.clientWriteKey, keys.clientWriteIV, keys.serverWriteKey, keys.serverWriteIV)
        } else {
            gcm, err = newCryptoGCM(keys.serverWriteKey, keys.serverWriteIV, keys.clientWriteKey, keys.clientWriteIV)
        }
        c.gcm.Store(gcm)

        return err

         */
        Ok(())
    }

    async fn encrypt(&self, pkt: &RecordLayer, raw: &[u8]) -> Result<Vec<u8>, Error> {
        let mut gcm = self.gcm.lock().await;
        if let Some(cg) = gcm.as_mut() {
            cg.encrypt(pkt, raw)
        } else {
            Err(Error::new(
                "CipherSuite has not been initialized, unable to encrypt".to_owned(),
            ))
        }
    }

    async fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        let mut gcm = self.gcm.lock().await;
        if let Some(cg) = gcm.as_mut() {
            cg.decrypt(input)
        } else {
            Err(Error::new(
                "CipherSuite has not been initialized, unable to decrypt".to_owned(),
            ))
        }
    }
}
