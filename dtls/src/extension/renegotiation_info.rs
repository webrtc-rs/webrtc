#[cfg(test)]
mod renegotiation_info_test;

use super::*;
use crate::error::Error::ErrInvalidPacketLength;

const RENEGOTIATION_INFO_HEADER_SIZE: usize = 5;

/// RenegotiationInfo allows a Client/Server to
/// communicate their renegotiation support
/// https://tools.ietf.org/html/rfc5746
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionRenegotiationInfo {
    pub(crate) renegotiated_connection: u8,
}

impl ExtensionRenegotiationInfo {
    // TypeValue returns the extension TypeValue
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::RenegotiationInfo
    }

    pub fn size(&self) -> usize {
        3
    }

    /// marshal encodes the extension
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BigEndian>(1)?; //length
        writer.write_u8(self.renegotiated_connection)?;

        Ok(writer.flush()?)
    }

    /// Unmarshal populates the extension from encoded data
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let l = reader.read_u16::<BigEndian>()?; //length
        if l != 1 {
            return Err(ErrInvalidPacketLength);
        }

        let renegotiated_connection = reader.read_u8()?;

        Ok(ExtensionRenegotiationInfo {
            renegotiated_connection,
        })
    }
}
