use super::*;
use crate::crypto::crypto_cbc::*;
use crate::prf::*;

#[derive(Clone)]
pub struct CipherSuiteAes256CbcSha {
    cbc: Option<CryptoCbc>,
    rsa: bool,
}

impl CipherSuiteAes256CbcSha {
    const PRF_MAC_LEN: usize = 20;
    const PRF_KEY_LEN: usize = 32;
    const PRF_IV_LEN: usize = 16;

    pub fn new(rsa: bool) -> Self {
        CipherSuiteAes256CbcSha { cbc: None, rsa }
    }
}

impl CipherSuite for CipherSuiteAes256CbcSha {
    fn to_string(&self) -> String {
        if self.rsa {
            "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA".to_owned()
        } else {
            "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA".to_owned()
        }
    }

    fn id(&self) -> CipherSuiteID {
        if self.rsa {
            CipherSuiteID::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA
        } else {
            CipherSuiteID::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
        }
    }

    fn certificate_type(&self) -> ClientCertificateType {
        if self.rsa {
            ClientCertificateType::RSASign
        } else {
            ClientCertificateType::ECDSASign
        }
    }

    fn hash_func(&self) -> CipherSuiteHash {
        CipherSuiteHash::SHA256
    }

    fn is_psk(&self) -> bool {
        false
    }

    fn is_initialized(&self) -> bool {
        self.cbc.is_some()
    }

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
            CipherSuiteAes256CbcSha::PRF_MAC_LEN,
            CipherSuiteAes256CbcSha::PRF_KEY_LEN,
            CipherSuiteAes256CbcSha::PRF_IV_LEN,
            self.hash_func(),
        )?;

        if is_client {
            self.cbc = Some(CryptoCbc::new(
                &keys.client_write_key,
                &keys.client_write_iv,
                &keys.client_mac_key,
                &keys.server_write_key,
                &keys.server_write_iv,
                &keys.server_mac_key,
            )?);
        } else {
            self.cbc = Some(CryptoCbc::new(
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

    fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(cg) = &self.cbc {
            cg.encrypt(pkt_rlh, raw)
        } else {
            Err(Error::new(
                "CipherSuite has not been initialized, unable to encrypt".to_owned(),
            ))
        }
    }

    fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(cg) = &self.cbc {
            cg.decrypt(input)
        } else {
            Err(Error::new(
                "CipherSuite has not been initialized, unable to decrypt".to_owned(),
            ))
        }
    }
}
