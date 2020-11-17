use super::*;
use crate::crypto::crypto_gcm::*;
use crate::prf::*;

//use std::sync::Arc;
//use tokio::sync::Mutex;
//use async_trait::async_trait;

#[derive(Clone)]
pub struct CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256 {
    gcm: Option<CryptoGcm>, //Arc<Mutex<Option<CryptoGcm>>>,
}

impl CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256 {
    const PRF_MAC_LEN: usize = 0;
    const PRF_KEY_LEN: usize = 16;
    const PRF_IV_LEN: usize = 4;
}

impl Default for CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256 {
    fn default() -> Self {
        CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256 {
            gcm: None, //Arc::new(Mutex::new(None)),
        }
    }
}

//#[async_trait]
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

    /*async*/
    fn is_initialized(&self) -> bool {
        //let gcm = self.gcm.lock().await;
        self.gcm.is_some()
    }

    /*async*/
    fn init(
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
            CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256::PRF_MAC_LEN,
            CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256::PRF_KEY_LEN,
            CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256::PRF_IV_LEN,
            self.hash_func(),
        )?;

        //let mut gcm = self.gcm.lock().await;
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

    /*async*/
    fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>, Error> {
        //let mut gcm = self.gcm.lock().await;
        if let Some(cg) = &self.gcm {
            cg.encrypt(pkt_rlh, raw)
        } else {
            Err(Error::new(
                "CipherSuite has not been initialized, unable to encrypt".to_owned(),
            ))
        }
    }

    /*async*/
    fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        //let mut gcm = self.gcm.lock().await;
        if let Some(cg) = &self.gcm {
            cg.decrypt(input)
        } else {
            Err(Error::new(
                "CipherSuite has not been initialized, unable to decrypt".to_owned(),
            ))
        }
    }
}
