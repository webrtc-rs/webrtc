use std::fmt;
use std::io::{BufReader, Read, Write};

use util::Error;

use super::header::*;

#[cfg(test)]
mod raw_packet_test;

// RawPacket represents an unparsed RTCP packet. It's returned by Unmarshal when
// a packet with an unknown type is encountered.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct RawPacket {
    pub header: Header,
    pub raw: Vec<u8>,
}

impl fmt::Display for RawPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawPacket: {:?}", self.raw)
    }
}

//var _ Packet = (*RawPacket)(nil) // assert is a Packet
impl RawPacket {
    fn size(&self) -> usize {
        self.raw.len()
    }

    // Unmarshal decodes the packet from binary.
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let mut raw_packet = RawPacket::default();
        reader.read_to_end(&mut raw_packet.raw)?;

        let mut reader = BufReader::new(raw_packet.raw.as_slice());
        raw_packet.header = Header::unmarshal(&mut reader)?;

        Ok(raw_packet)
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        self.header.clone()
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    pub fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    // Marshal encodes the packet in binary.
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_all(&self.raw)?;
        Ok(writer.flush()?)
    }
}
