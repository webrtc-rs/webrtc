#[cfg(test)]
mod extension_use_extended_master_secret_test;

use super::*;

const EXTENSION_USE_EXTENDED_MASTER_SECRET_HEADER_SIZE: usize = 4;

// https://tools.ietf.org/html/rfc8422
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionUseExtendedMasterSecret {
    pub(crate) supported: bool,
}

impl ExtensionUseExtendedMasterSecret {
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::UseExtendedMasterSecret
    }

    pub fn size(&self) -> usize {
        2
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        // length
        writer.write_u16::<BigEndian>(0)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let _ = reader.read_u16::<BigEndian>()?;

        Ok(ExtensionUseExtendedMasterSecret { supported: true })
    }
}
