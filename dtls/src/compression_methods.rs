use std::io::{Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

use crate::error::Result;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CompressionMethodId {
    Null = 0,
    Unsupported,
}

impl From<u8> for CompressionMethodId {
    fn from(val: u8) -> Self {
        match val {
            0 => CompressionMethodId::Null,
            _ => CompressionMethodId::Unsupported,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompressionMethods {
    pub ids: Vec<CompressionMethodId>,
}

impl CompressionMethods {
    pub fn size(&self) -> usize {
        1 + self.ids.len()
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u8(self.ids.len() as u8)?;

        for id in &self.ids {
            writer.write_u8(*id as u8)?;
        }

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let compression_methods_count = reader.read_u8()? as usize;
        let mut ids = vec![];
        for _ in 0..compression_methods_count {
            let id = reader.read_u8()?.into();
            if id != CompressionMethodId::Unsupported {
                ids.push(id);
            }
        }

        Ok(CompressionMethods { ids })
    }
}

pub fn default_compression_methods() -> CompressionMethods {
    CompressionMethods {
        ids: vec![CompressionMethodId::Null],
    }
}
