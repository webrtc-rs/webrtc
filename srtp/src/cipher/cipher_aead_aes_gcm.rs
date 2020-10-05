//use super::*;

pub(crate) const CIPHER_AEAD_AES_GCM_AUTH_TAG_LEN: usize = 16;

/*
const RTCP_ENCRYPTION_FLAG: u8 = 0x80;
pub(crate) struct CipherAeadAesGcm {
    //srtpCipher, srtcpCipher cipher.AEAD
//srtpSessionSalt, srtcpSessionSalt []byte
}

impl CipherAeadAesGcm {
    pub fn new(master_key: &[u8], master_salt: &[u8]) -> Result<Self, Error> {
        unimplemented!()
    }
}

impl Cipher for CipherAeadAesGcm {
    fn auth_tag_len(&self) -> usize {
        CIPHER_AEAD_AES_GCM_AUTH_TAG_LEN
    }

    fn get_rtcp_index(&self, a: &[u8]) -> Result<u32, Error> {
        Ok(0)
    }

    fn encrypt_rtp(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(vec![])
    }
    fn decrypt_rtp(&mut self, encrypted: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(vec![])
    }

    fn decrypt_rtcp(&mut self, encrypted: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(vec![])
    }
    fn encrypt_rtcp(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(vec![])
    }
}
*/
