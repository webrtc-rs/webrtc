#[cfg(test)]
mod extension_supported_point_formats_test;

use super::*;

const EXTENSION_SUPPORTED_POINT_FORMATS_SIZE: usize = 5;

pub type EllipticCurvePointFormat = u8;

pub const ELLIPTIC_CURVE_POINT_FORMAT_UNCOMPRESSED: EllipticCurvePointFormat = 0;

// https://tools.ietf.org/html/rfc4492#section-5.1.2
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionSupportedPointFormats {
    pub(crate) point_formats: Vec<EllipticCurvePointFormat>,
}

impl ExtensionSupportedPointFormats {
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::SupportedPointFormats
    }

    pub fn size(&self) -> usize {
        2 + 1 + self.point_formats.len()
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BigEndian>(1 + self.point_formats.len() as u16)?;
        writer.write_u8(self.point_formats.len() as u8)?;
        for v in &self.point_formats {
            writer.write_u8(*v)?;
        }

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let _ = reader.read_u16::<BigEndian>()?;

        let point_format_count = reader.read_u8()? as usize;
        let mut point_formats = vec![];
        for _ in 0..point_format_count {
            let point_format = reader.read_u8()?;
            point_formats.push(point_format);
        }

        Ok(ExtensionSupportedPointFormats { point_formats })
    }
}
