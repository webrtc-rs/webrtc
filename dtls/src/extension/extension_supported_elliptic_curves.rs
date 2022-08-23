#[cfg(test)]
mod extension_supported_elliptic_curves_test;

use super::*;
use crate::curve::named_curve::*;

const EXTENSION_SUPPORTED_GROUPS_HEADER_SIZE: usize = 6;

// https://tools.ietf.org/html/rfc8422#section-5.1.1
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionSupportedEllipticCurves {
    pub elliptic_curves: Vec<NamedCurve>,
}

impl ExtensionSupportedEllipticCurves {
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::SupportedEllipticCurves
    }

    pub fn size(&self) -> usize {
        2 + 2 + self.elliptic_curves.len() * 2
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BigEndian>(2 + 2 * self.elliptic_curves.len() as u16)?;
        writer.write_u16::<BigEndian>(2 * self.elliptic_curves.len() as u16)?;
        for v in &self.elliptic_curves {
            writer.write_u16::<BigEndian>(*v as u16)?;
        }

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let _ = reader.read_u16::<BigEndian>()?;

        let group_count = reader.read_u16::<BigEndian>()? as usize / 2;
        let mut elliptic_curves = vec![];
        for _ in 0..group_count {
            let elliptic_curve = reader.read_u16::<BigEndian>()?.into();
            elliptic_curves.push(elliptic_curve);
        }

        Ok(ExtensionSupportedEllipticCurves { elliptic_curves })
    }
}
