use super::*;

use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use util::Error;

// msg_len for Handshake messages assumes an extra 12 bytes for
// sequence, fragment and version information
const HANDSHAKE_HEADER_LENGTH: usize = 12;

pub struct HandshakeHeader {
    pub handshake_type: HandshakeType,
    length: u32, // uint24 in spec
    message_sequence: u16,
    fragment_offset: u32, // uint24 in spec
    fragment_length: u32, // uint24 in spec
}

impl HandshakeHeader {
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_u8(self.handshake_type as u8)?;
        writer.write_u24::<BigEndian>(self.length)?;
        writer.write_u16::<BigEndian>(self.message_sequence)?;
        writer.write_u24::<BigEndian>(self.fragment_offset)?;
        writer.write_u24::<BigEndian>(self.fragment_length)?;

        Ok(())
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
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
