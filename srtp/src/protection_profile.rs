use util::Error;

// ProtectionProfile specifies Cipher and AuthTag details, similar to TLS cipher suite
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProtectionProfile(u16);

// Supported protection profiles
pub const PROTECTION_PROFILE_AES128CM_HMAC_SHA1_80: ProtectionProfile = ProtectionProfile(0x0001);
pub const PROTECTION_PROFILE_AEAD_AES128_GCM: ProtectionProfile = ProtectionProfile(0x0007);

impl ProtectionProfile {
    pub(crate) fn key_len(&self) -> Result<usize, Error> {
        match *self {
            PROTECTION_PROFILE_AES128CM_HMAC_SHA1_80 | PROTECTION_PROFILE_AEAD_AES128_GCM => Ok(16),
            p => Err(Error::new(format!("no such ProtectionProfile {}", p.0))),
        }
    }

    pub(crate) fn salt_len(&self) -> Result<usize, Error> {
        match *self {
            PROTECTION_PROFILE_AES128CM_HMAC_SHA1_80 => Ok(14),
            PROTECTION_PROFILE_AEAD_AES128_GCM => Ok(12),
            p => Err(Error::new(format!("no such ProtectionProfile {}", p.0))),
        }
    }

    /*pub(crate) fn authTagLen(&self) -> Result<isize, Error> {
        match *self {
            PROTECTION_PROFILE_AES128CM_HMAC_SHA1_80=>
            return (&srtpCipherAesCmHmacSha1{}).authTagLen(), nil
                PROTECTION_PROFILE_AEAD_AES128_GCM =>
            return (&srtpCipherAeadAesGcm{}).authTagLen(), nil
            p => Err(Error::new(format!("no such ProtectionProfile {}", p.0))),
        }
    }*/

    pub(crate) fn auth_key_len(&self) -> Result<usize, Error> {
        match *self {
            PROTECTION_PROFILE_AES128CM_HMAC_SHA1_80 => Ok(20),
            PROTECTION_PROFILE_AEAD_AES128_GCM => Ok(0),
            p => Err(Error::new(format!("no such ProtectionProfile {}", p.0))),
        }
    }
}
