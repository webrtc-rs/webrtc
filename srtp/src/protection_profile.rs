/// ProtectionProfile specifies Cipher and AuthTag details, similar to TLS cipher suite
#[derive(Default, Debug, Clone, Copy)]
#[repr(u8)]
pub enum ProtectionProfile {
    #[default]
    Aes128CmHmacSha1_80 = 0x0001,
    AeadAes128Gcm = 0x0007,
}

impl ProtectionProfile {
    pub(crate) fn key_len(&self) -> usize {
        match *self {
            ProtectionProfile::Aes128CmHmacSha1_80 | ProtectionProfile::AeadAes128Gcm => 16,
        }
    }

    pub(crate) fn salt_len(&self) -> usize {
        match *self {
            ProtectionProfile::Aes128CmHmacSha1_80 => 14,
            ProtectionProfile::AeadAes128Gcm => 12,
        }
    }

    pub(crate) fn auth_tag_len(&self) -> usize {
        match *self {
            ProtectionProfile::Aes128CmHmacSha1_80 => 10, //CIPHER_AES_CM_HMAC_SHA1AUTH_TAG_LEN,
            ProtectionProfile::AeadAes128Gcm => 16,       //CIPHER_AEAD_AES_GCM_AUTH_TAG_LEN,
        }
    }

    pub(crate) fn auth_key_len(&self) -> usize {
        match *self {
            ProtectionProfile::Aes128CmHmacSha1_80 => 20,
            ProtectionProfile::AeadAes128Gcm => 0,
        }
    }
}
