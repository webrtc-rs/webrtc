use hmac::{Hmac, Mac};
use sha1::Sha1;

use super::context::*;
use super::key_derivation::*;
use super::protection_profile::*;
use util::Error;

type HmacSha1 = Hmac<Sha1>;

pub(crate) struct CipherAesCmHmacSha1 {
    srtp_session_key: Vec<u8>,
    srtp_session_salt: Vec<u8>,
    srtp_session_auth: HmacSha1,
    srtp_session_auth_tag: Vec<u8>,

    srtcp_session_key: Vec<u8>,
    srtcp_session_salt: Vec<u8>,
    srtcp_session_auth: HmacSha1,
    srtcp_session_auth_tag: Vec<u8>,
}

impl CipherAesCmHmacSha1 {
    pub fn new(master_key: &[u8], master_salt: &[u8]) -> Result<Self, Error> {
        let srtp_session_key = aes_cm_key_derivation(
            LABEL_SRTP_ENCRYPTION,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;
        let srtcp_session_key = aes_cm_key_derivation(
            LABEL_SRTCP_ENCRYPTION,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;

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

        let auth_key_len = PROTECTION_PROFILE_AES128CM_HMAC_SHA1_80.auth_key_len()?;

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

        let srtp_session_auth = match HmacSha1::new_varkey(&srtp_session_auth_tag) {
            Ok(srtp_session_auth) => srtp_session_auth,
            Err(err) => return Err(Error::new(err.to_string())),
        };
        let srtcp_session_auth = match HmacSha1::new_varkey(&srtcp_session_auth_tag) {
            Ok(srtcp_session_auth) => srtcp_session_auth,
            Err(err) => return Err(Error::new(err.to_string())),
        };

        Ok(CipherAesCmHmacSha1 {
            srtp_session_key,
            srtp_session_salt,
            srtp_session_auth,
            srtp_session_auth_tag,

            srtcp_session_key,
            srtcp_session_salt,
            srtcp_session_auth,
            srtcp_session_auth_tag,
        })
    }

    pub(crate) fn auth_tag_len() -> usize {
        10
    }
}
