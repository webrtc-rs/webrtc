pub mod cipher_aead_aes_gcm;
pub mod cipher_aes_cm_hmac_sha1;
mod test;

pub(crate) use cipher_aead_aes_gcm::CipherAeadAesGcm;
pub(crate) use cipher_aes_cm_hmac_sha1::CipherAesCmHmacSha1;

use util::Error;

pub(crate) trait Cipher {
    fn auth_tag_len(&self) -> usize;

    fn get_rtcp_index(&self, input: &[u8]) -> usize;

    fn encrypt_rtp(
        &mut self,
        payload: &[u8],
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Vec<u8>, Error>;

    fn decrypt_rtp(
        &mut self,
        encrypted: &[u8],
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Vec<u8>, Error>;

    fn encrypt_rtcp(
        &mut self,
        decrypted: &[u8],
        srtcp_index: usize,
        ssrc: u32,
    ) -> Result<Vec<u8>, Error>;

    fn decrypt_rtcp(
        &mut self,
        encrypted: &[u8],
        srtcp_index: usize,
        ssrc: u32,
    ) -> Result<Vec<u8>, Error>;
}
