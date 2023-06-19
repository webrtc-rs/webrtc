use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::*;

// msg_len for Handshake messages assumes an extra 12 bytes for
// sequence, Fragment and version information
pub(crate) const HANDSHAKE_HEADER_LENGTH: usize = 12;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct HandshakeHeader {
    pub(crate) handshake_type: HandshakeType,
    pub(crate) length: u32, // uint24 in spec
    pub(crate) message_sequence: u16,
    pub(crate) fragment_offset: u32, // uint24 in spec
    pub(crate) fragment_length: u32, // uint24 in spec
}

impl HandshakeHeader {
    pub fn size(&self) -> usize {
        1 + 3 + 2 + 3 + 3
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u8(self.handshake_type as u8)?;
        writer.write_u24::<BigEndian>(self.length)?;
        writer.write_u16::<BigEndian>(self.message_sequence)?;
        writer.write_u24::<BigEndian>(self.fragment_offset)?;
        writer.write_u24::<BigEndian>(self.fragment_length)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let handshake_type = reader.read_u8()?.into();
        let length = reader.read_u24::<BigEndian>()?;
        let message_sequence = reader.read_u16::<BigEndian>()?;
        let fragment_offset = reader.read_u24::<BigEndian>()?;
        let fragment_length = reader.read_u24::<BigEndian>()?;

        Ok(HandshakeHeader {
            handshake_type,
            length,
            message_sequence,
            fragment_offset,
            fragment_length,
        })
    }
}
