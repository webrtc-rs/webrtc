use super::cipher::cipher_aead_aes_gcm::*;
use super::cipher::cipher_aes_cm_hmac_sha1::*;
mod test;

/// ProtectionProfile specifies Cipher and AuthTag details, similar to TLS cipher suite
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ProtectionProfile {
    AES128CMHMACSHA1_80 = 0x0001,
    AEADAES128GCM = 0x0007,
}

impl ProtectionProfile {
    pub(crate) fn key_len(&self) -> usize {
        match *self {
            ProtectionProfile::AES128CMHMACSHA1_80 | ProtectionProfile::AEADAES128GCM => 16,
        }
    }

    pub(crate) fn salt_len(&self) -> usize {
        match *self {
            ProtectionProfile::AES128CMHMACSHA1_80 => 14,
            ProtectionProfile::AEADAES128GCM => 12,
        }
    }

    pub(crate) fn auth_tag_len(&self) -> usize {
        match *self {
            ProtectionProfile::AES128CMHMACSHA1_80 => CIPHER_AES_CM_HMAC_SHA1AUTH_TAG_LEN,
            ProtectionProfile::AEADAES128GCM => CIPHER_AEAD_AES_GCM_AUTH_TAG_LEN,
        }
    }

    pub(crate) fn auth_key_len(&self) -> usize {
        match *self {
            ProtectionProfile::AES128CMHMACSHA1_80 => 20,
            ProtectionProfile::AEADAES128GCM => 0,
        }
    }
}
