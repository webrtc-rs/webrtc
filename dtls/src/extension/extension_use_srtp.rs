use super::*;

// SRTPProtectionProfile defines the parameters and options that are in effect for the SRTP processing
// https://tools.ietf.org/html/rfc5764#section-4.1.2
#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq)]
pub enum SRTPProtectionProfile {
    SRTP_AES128_CM_HMAC_SHA1_80 = 0x0001,
    SRTP_AES128_CM_HMAC_SHA1_32 = 0x0002,
    SRTP_AEAD_AES_128_GCM = 0x0007,
    SRTP_AEAD_AES_256_GCM = 0x0008,
}

const EXTENSION_USE_SRTPHEADER_SIZE: usize = 6;

// https://tools.ietf.org/html/rfc8422
#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionUseSRTP {
    protection_profiles: Vec<SRTPProtectionProfile>,
}

impl ExtensionUseSRTP {
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::UseSRTP
    }
}
