use util::Error;

pub(crate) mod cipher_aead_aes_gcm;
pub(crate) mod cipher_aes_cm_hmac_sha1;

pub(crate) trait Cipher {
    fn auth_tag_len(&self) -> usize;
    fn get_rtcp_index(&self, input: &[u8]) -> Result<u32, Error>;

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
        srtcp_index: u32,
        ssrc: u32,
    ) -> Result<Vec<u8>, Error>;
    fn decrypt_rtcp(
        &mut self,
        encrypted: &[u8],
        srtcp_index: u32,
        ssrc: u32,
    ) -> Result<Vec<u8>, Error>;
}
