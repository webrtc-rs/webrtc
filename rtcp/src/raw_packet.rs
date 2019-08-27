use std::fmt;
use std::io::{BufReader, Read, Write};

use utils::Error;

use super::errors::*;
use super::header::*;
use super::packet::*;

#[cfg(test)]
mod raw_packet_test;

// RawPacket represents an unparsed RTCP packet. It's returned by Unmarshal when
// a packet with an unknown type is encountered.
#[derive(Debug, PartialEq, Default)]
pub struct RawPacket(Vec<u8>);

impl fmt::Display for RawPacket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RawPacket: {:?}", self.0)
    }
}

//var _ Packet = (*RawPacket)(nil) // assert is a Packet
impl RawPacket {
    fn len(&self) -> usize {
        self.0.len()
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Result<Header, Error> {
        let mut reader = BufReader::new(self.0.as_slice());
        Header::unmarshal(&mut reader)
    }

    // Unmarshal decodes the packet from binary.
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let mut raw_packet = RawPacket(vec![]);
        reader.read_to_end(&mut raw_packet.0)?;

        if raw_packet.0.len() < HEADER_LENGTH {
            Err(ErrPacketTooShort.clone())
        } else {
            raw_packet.header()?;
            Ok(raw_packet)
        }
    }
}

impl<W: Write> Packet<W> for RawPacket {
    // DestinationSSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    // Marshal encodes the packet in binary.
    fn marshal(&self, writer: &mut W) -> Result<(), Error> {
        writer.write(&self.0)?;
        Ok(())
    }
}
