use util::Error;

use std::io::{Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

#[derive(Copy, Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct CompressionMethods {
    ids: Vec<CompressionMethodId>,
}

impl CompressionMethods {
    fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
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

    fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_u8(self.ids.len() as u8)?;

        for id in &self.ids {
            writer.write_u8(*id as u8)?;
        }

        Ok(())
    }
}
