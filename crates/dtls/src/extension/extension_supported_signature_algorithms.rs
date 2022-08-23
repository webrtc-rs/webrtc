#[cfg(test)]
mod extension_supported_signature_algorithms_test;

use super::*;
use crate::signature_hash_algorithm::*;

const EXTENSION_SUPPORTED_SIGNATURE_ALGORITHMS_HEADER_SIZE: usize = 6;

// https://tools.ietf.org/html/rfc5246#section-7.4.1.4.1
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionSupportedSignatureAlgorithms {
    pub(crate) signature_hash_algorithms: Vec<SignatureHashAlgorithm>,
}

impl ExtensionSupportedSignatureAlgorithms {
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::SupportedSignatureAlgorithms
    }

    pub fn size(&self) -> usize {
        2 + 2 + self.signature_hash_algorithms.len() * 2
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BigEndian>(2 + 2 * self.signature_hash_algorithms.len() as u16)?;
        writer.write_u16::<BigEndian>(2 * self.signature_hash_algorithms.len() as u16)?;
        for v in &self.signature_hash_algorithms {
            writer.write_u8(v.hash as u8)?;
            writer.write_u8(v.signature as u8)?;
        }

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let _ = reader.read_u16::<BigEndian>()?;

        let algorithm_count = reader.read_u16::<BigEndian>()? as usize / 2;
        let mut signature_hash_algorithms = vec![];
        for _ in 0..algorithm_count {
            let hash = reader.read_u8()?.into();
            let signature = reader.read_u8()?.into();
            signature_hash_algorithms.push(SignatureHashAlgorithm { hash, signature });
        }

        Ok(ExtensionSupportedSignatureAlgorithms {
            signature_hash_algorithms,
        })
    }
}
