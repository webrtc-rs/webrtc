#[cfg(test)]
mod handshake_message_finished_test;

use std::io::{Read, Write};

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeMessageFinished {
    pub(crate) verify_data: Vec<u8>,
}

impl HandshakeMessageFinished {
    pub fn handshake_type(&self) -> HandshakeType {
        HandshakeType::Finished
    }

    pub fn size(&self) -> usize {
        self.verify_data.len()
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_all(&self.verify_data)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let mut verify_data: Vec<u8> = vec![];
        reader.read_to_end(&mut verify_data)?;

        Ok(HandshakeMessageFinished { verify_data })
    }
}
