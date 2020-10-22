use super::*;

use std::sync::{Arc, Mutex};

const PRF_MAC_LEN: usize = 0;
const PRF_KEY_LEN: usize = 16;
const PRF_IV_LEN: usize = 4;

pub struct CipherSuiteTLSEcdheEcdsaWithAes128GcmSha256 {
    gcm: Arc<Mutex<Option<bool>>>,
}

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

    fn is_initialized(&self) -> bool {
        if let Ok(gcm) = self.gcm.lock() {
            gcm.is_some()
        } else {
            false
        }
    }

    fn init(
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

    fn encrypt(&self, _pkt: &RecordLayer, _raw: &[u8]) -> Result<Vec<u8>, Error> {
        /*gcm := c.gcm.Load()
        if gcm == nil { // !c.isInitialized()
            return nil, errors.New("CipherSuite has not been initialized, unable to encrypt")
        }

        return gcm.(*CryptoGcm).encrypt(pkt, raw)

         */
        Ok(vec![])
    }

    fn decrypt(&self, _input: &[u8]) -> Result<Vec<u8>, Error> {
        /*gcm := c.gcm.Load()
        if gcm == nil { // !c.isInitialized()
            return nil, errors.New("CipherSuite has not been initialized, unable to decrypt ")
        }

        return gcm.(*CryptoGcm).decrypt(raw)*/
        Ok(vec![])
    }
}
