use byteorder::{BigEndian, ByteOrder};
use hmac::{Hmac, Mac};
use sha1::Sha1;

use super::Cipher;
use crate::error::{Error, Result};
use crate::key_derivation::*;
use crate::protection_profile::*;

#[cfg(not(feature = "openssl"))]
mod ctrcipher;

#[cfg(feature = "openssl")]
mod opensslcipher;

#[cfg(not(feature = "openssl"))]
pub(crate) use ctrcipher::CipherAesCmHmacSha1;

#[cfg(feature = "openssl")]
pub(crate) use opensslcipher::CipherAesCmHmacSha1;

type HmacSha1 = Hmac<Sha1>;

pub const CIPHER_AES_CM_HMAC_SHA1AUTH_TAG_LEN: usize = 10;

pub(crate) struct CipherInner {
    profile: ProtectionProfile,
    srtp_session_salt: Vec<u8>,
    srtp_session_auth: HmacSha1,
    srtcp_session_salt: Vec<u8>,
    srtcp_session_auth: HmacSha1,
}

impl CipherInner {
    pub fn new(profile: ProtectionProfile, master_key: &[u8], master_salt: &[u8]) -> Result<Self> {
        let srtp_session_salt = aes_cm_key_derivation(
            LABEL_SRTP_SALT,
            master_key,
            master_salt,
            0,
            master_salt.len(),
        )?;
        let srtcp_session_salt = aes_cm_key_derivation(
            LABEL_SRTCP_SALT,
            master_key,
            master_salt,
            0,
            master_salt.len(),
        )?;

        let auth_key_len = ProtectionProfile::Aes128CmHmacSha1_80.auth_key_len();

        let srtp_session_auth_tag = aes_cm_key_derivation(
            LABEL_SRTP_AUTHENTICATION_TAG,
            master_key,
            master_salt,
            0,
            auth_key_len,
        )?;
        let srtcp_session_auth_tag = aes_cm_key_derivation(
            LABEL_SRTCP_AUTHENTICATION_TAG,
            master_key,
            master_salt,
            0,
            auth_key_len,
        )?;

        let srtp_session_auth = HmacSha1::new_from_slice(&srtp_session_auth_tag)
            .map_err(|e| Error::Other(e.to_string()))?;
        let srtcp_session_auth = HmacSha1::new_from_slice(&srtcp_session_auth_tag)
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(Self {
            profile,
            srtp_session_salt,
            srtp_session_auth,
            srtcp_session_salt,
            srtcp_session_auth,
        })
    }

    /// https://tools.ietf.org/html/rfc3711#section-4.2
    /// In the case of SRTP, M SHALL consist of the Authenticated
    /// Portion of the packet (as specified in Figure 1) concatenated with
    /// the roc, M = Authenticated Portion || roc;
    ///
    /// The pre-defined authentication transform for SRTP is HMAC-SHA1
    /// [RFC2104].  With HMAC-SHA1, the SRTP_PREFIX_LENGTH (Figure 3) SHALL
    /// be 0.  For SRTP (respectively SRTCP), the HMAC SHALL be applied to
    /// the session authentication key and M as specified above, i.e.,
    /// HMAC(k_a, M).  The HMAC output SHALL then be truncated to the n_tag
    /// left-most bits.
    /// - Authenticated portion of the packet is everything BEFORE MKI
    /// - k_a is the session message authentication key
    /// - n_tag is the bit-length of the output authentication tag
    fn generate_srtp_auth_tag(&self, buf: &[u8], roc: u32) -> [u8; 20] {
        let mut signer = self.srtp_session_auth.clone();

        signer.update(buf);

        // For SRTP only, we need to hash the rollover counter as well.
        signer.update(&roc.to_be_bytes());

        signer.finalize().into_bytes().into()
    }

    /// https://tools.ietf.org/html/rfc3711#section-4.2
    ///
    /// The pre-defined authentication transform for SRTP is HMAC-SHA1
    /// [RFC2104].  With HMAC-SHA1, the SRTP_PREFIX_LENGTH (Figure 3) SHALL
    /// be 0.  For SRTP (respectively SRTCP), the HMAC SHALL be applied to
    /// the session authentication key and M as specified above, i.e.,
    /// HMAC(k_a, M).  The HMAC output SHALL then be truncated to the n_tag
    /// left-most bits.
    /// - Authenticated portion of the packet is everything BEFORE MKI
    /// - k_a is the session message authentication key
    /// - n_tag is the bit-length of the output authentication tag
    fn generate_srtcp_auth_tag(&self, buf: &[u8]) -> [u8; 20] {
        let mut signer = self.srtcp_session_auth.clone();

        signer.update(buf);

        signer.finalize().into_bytes().into()
    }

    fn get_rtcp_index(&self, input: &[u8]) -> usize {
        let tail_offset = input.len() - (self.profile.rtcp_auth_tag_len() + SRTCP_INDEX_SIZE);
        (BigEndian::read_u32(&input[tail_offset..tail_offset + SRTCP_INDEX_SIZE]) & !(1 << 31))
            as usize
    }
}
