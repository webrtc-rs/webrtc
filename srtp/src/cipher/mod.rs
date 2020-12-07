pub mod cipher_aead_aes_gcm;
pub mod cipher_aes_cm_hmac_sha1;
mod test;

use bytes::BytesMut;
pub(crate) use cipher_aead_aes_gcm::CipherAeadAesGcm;
pub(crate) use cipher_aes_cm_hmac_sha1::CipherAesCmHmacSha1;

use util::Error;

/// Cipher represents a implementation of one
/// of the SRTP Specific ciphers.
pub(crate) trait Cipher {
    /// Get authenticated tag length.
    fn auth_tag_len(&self) -> usize;

    /// Retrieved RTCP index.
    fn get_rtcp_index(&self, input: &[u8]) -> usize;

    /// Encrypt RTP payload.
    fn encrypt_rtp(
        &mut self,
        payload: &BytesMut,
        header: &mut rtp::header::Header,
        roc: u32,
    ) -> Result<BytesMut, Error>;

    /// Decrypt RTP encrypted payload.
    fn decrypt_rtp(
        &mut self,
        encrypted: &BytesMut,
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<BytesMut, Error>;

    /// Encrypt RTCP payload.
    fn encrypt_rtcp(
        &mut self,
        decrypted: &BytesMut,
        srtcp_index: usize,
        ssrc: u32,
    ) -> Result<BytesMut, Error>;

    /// Decrypt RTCP encrypted payload.
    fn decrypt_rtcp(
        &mut self,
        encrypted: &BytesMut,
        srtcp_index: usize,
        ssrc: u32,
    ) -> Result<BytesMut, Error>;
}
