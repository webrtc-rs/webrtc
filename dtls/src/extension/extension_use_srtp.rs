#[cfg(test)]
mod extension_use_srtp_test;

use super::*;

// SRTPProtectionProfile defines the parameters and options that are in effect for the SRTP processing
// https://tools.ietf.org/html/rfc5764#section-4.1.2
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SrtpProtectionProfile {
    Srtp_Aes128_Cm_Hmac_Sha1_80 = 0x0001,
    Srtp_Aes128_Cm_Hmac_Sha1_32 = 0x0002,
    Srtp_Aead_Aes_128_Gcm = 0x0007,
    Srtp_Aead_Aes_256_Gcm = 0x0008,
    Unsupported,
}

impl From<u16> for SrtpProtectionProfile {
    fn from(val: u16) -> Self {
        match val {
            0x0001 => SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80,
            0x0002 => SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_32,
            0x0007 => SrtpProtectionProfile::Srtp_Aead_Aes_128_Gcm,
            0x0008 => SrtpProtectionProfile::Srtp_Aead_Aes_256_Gcm,
            _ => SrtpProtectionProfile::Unsupported,
        }
    }
}

const EXTENSION_USE_SRTPHEADER_SIZE: usize = 6;

// https://tools.ietf.org/html/rfc8422
#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionUseSrtp {
    pub(crate) protection_profiles: Vec<SrtpProtectionProfile>,
}

impl ExtensionUseSrtp {
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::UseSrtp
    }

    pub fn size(&self) -> usize {
        2 + 2 + self.protection_profiles.len() * 2 + 1
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
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

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let _ = reader.read_u16::<BigEndian>()?;

        let profile_count = reader.read_u16::<BigEndian>()? as usize / 2;
        let mut protection_profiles = vec![];
        for _ in 0..profile_count {
            let protection_profile = reader.read_u16::<BigEndian>()?.into();
            protection_profiles.push(protection_profile);
        }

        /* MKI Length */
        let _ = reader.read_u8()?;

        Ok(ExtensionUseSrtp {
            protection_profiles,
        })
    }
}
