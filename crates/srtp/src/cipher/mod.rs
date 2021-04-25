pub mod cipher_aead_aes_gcm;
pub mod cipher_aes_cm_hmac_sha1;

use crate::error::Error;

use bytes::Bytes;

///NOTE: Auth tag and AEAD auth tag are placed at the different position in SRTCP
///
///In non-AEAD cipher, the authentication tag is placed *after* the ESRTCP word
///(Encrypted-flag and SRTCP index).
///
///> AES_128_CM_HMAC_SHA1_80
///> | RTCP Header | Encrypted payload |E| SRTCP Index | Auth tag |
///>                                   ^               |----------|
///>                                   |                ^
///>                                   |                authTagLen=10
///>                                   aeadAuthTagLen=0
///
///In AEAD cipher, the AEAD authentication tag is embedded in the ciphertext.
///It is *before* the ESRTCP word (Encrypted-flag and SRTCP index).
///
///> AEAD_AES_128_GCM
///> | RTCP Header | Encrypted payload | AEAD auth tag |E| SRTCP Index |
///>                                   |---------------|               ^
///>                                    ^                              authTagLen=0
///>                                    aeadAuthTagLen=16
///
///See https://tools.ietf.org/html/rfc7714 for the full specifications.

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
