use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::error::Error;

#[cfg(test)]
mod transport_cc_extension_test;

// TransportCCExtension is a extension payload format in
// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01
// 0                   1                   2                   3
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |       0xBE    |    0xDE       |           length=1            |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |  ID   | L=1   |transport-wide sequence number | zero padding  |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(PartialEq, Debug)]
pub struct TransportCCExtension {
    transport_sequence: u16,
}

impl TransportCCExtension {
    // Marshal serializes the members to buffer
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_u16::<BigEndian>(self.transport_sequence)?;

        Ok(writer.flush()?)
    }

    // Unmarshal parses the passed byte slice and stores the result in the members
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let transport_sequence = reader.read_u16::<BigEndian>()?;

        Ok(TransportCCExtension { transport_sequence })
    }
}
