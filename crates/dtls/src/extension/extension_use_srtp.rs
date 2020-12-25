#[cfg(test)]
mod extension_use_srtp_test;

use super::*;

// SRTPProtectionProfile defines the parameters and options that are in effect for the SRTP processing
// https://tools.ietf.org/html/rfc5764#section-4.1.2
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SRTPProtectionProfile {
    SRTP_AES128_CM_HMAC_SHA1_80 = 0x0001,
    SRTP_AES128_CM_HMAC_SHA1_32 = 0x0002,
    SRTP_AEAD_AES_128_GCM = 0x0007,
    SRTP_AEAD_AES_256_GCM = 0x0008,
    Unsupported,
}

impl From<u16> for SRTPProtectionProfile {
    fn from(val: u16) -> Self {
        match val {
            0x0001 => SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_80,
            0x0002 => SRTPProtectionProfile::SRTP_AES128_CM_HMAC_SHA1_32,
            0x0007 => SRTPProtectionProfile::SRTP_AEAD_AES_128_GCM,
            0x0008 => SRTPProtectionProfile::SRTP_AEAD_AES_256_GCM,
            _ => SRTPProtectionProfile::Unsupported,
        }
    }
}

const EXTENSION_USE_SRTPHEADER_SIZE: usize = 6;

// https://tools.ietf.org/html/rfc8422
#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionUseSRTP {
    pub(crate) protection_profiles: Vec<SRTPProtectionProfile>,
}

impl ExtensionUseSRTP {
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::UseSRTP
    }

    pub fn size(&self) -> usize {
        2 + 2 + self.protection_profiles.len() * 2 + 1
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_u16::<BigEndian>(
            2 + /* MKI Length */ 1 + 2 * self.protection_profiles.len() as u16,
        )?;
        writer.write_u16::<BigEndian>(2 * self.protection_profiles.len() as u16)?;
        for v in &self.protection_profiles {
            writer.write_u16::<BigEndian>(*v as u16)?;
        }

        /* MKI Length */
        writer.write_u8(0x00)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let _ = reader.read_u16::<BigEndian>()?;

        let profile_count = reader.read_u16::<BigEndian>()? as usize / 2;
        let mut protection_profiles = vec![];
        for _ in 0..profile_count {
            let protection_profile = reader.read_u16::<BigEndian>()?.into();
            protection_profiles.push(protection_profile);
        }

        /* MKI Length */
        let _ = reader.read_u8()?;

        Ok(ExtensionUseSRTP {
            protection_profiles,
        })
    }
}
