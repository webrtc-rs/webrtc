pub mod cipher_aead_aes_gcm;
pub mod cipher_aes_cm_hmac_sha1;

use crate::error::Error;

use bytes::Bytes;

/// Cipher represents a implementation of one
/// of the SRTP Specific ciphers.
pub(crate) trait Cipher {
    /// Get authenticated tag length.
    fn auth_tag_len(&self) -> usize;

    /// Retrieved RTCP index.
    fn get_rtcp_index(&self, input: &Bytes) -> usize;

    /// Encrypt RTP payload.
    fn encrypt_rtp(
        &mut self,
        payload: &Bytes,
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Bytes, Error>;

    /// Decrypt RTP encrypted payload.
    fn decrypt_rtp(
        &mut self,
        encrypted: &Bytes,
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Bytes, Error>;

    /// Encrypt RTCP payload.
    fn encrypt_rtcp(
        &mut self,
        decrypted: &Bytes,
        srtcp_index: usize,
        ssrc: u32,
    ) -> Result<Bytes, Error>;

    /// Decrypt RTCP encrypted payload.
    fn decrypt_rtcp(
        &mut self,
        encrypted: &Bytes,
        srtcp_index: usize,
        ssrc: u32,
    ) -> Result<Bytes, Error>;
}
