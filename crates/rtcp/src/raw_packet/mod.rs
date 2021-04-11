#[cfg(test)]
mod raw_packet_test;

use crate::{error::Error, header::*, packet::Packet};

use bytes::Bytes;
use std::fmt;

/// RawPacket represents an unparsed RTCP packet. It's returned by Unmarshal when
/// a packet with an unknown type is encountered.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct RawPacket(pub Bytes);

impl fmt::Display for RawPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawPacket: {:?}", self)
    }
}

impl Packet for RawPacket {
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn marshal_size(&self) -> usize {
        self.0.len()
    }

    /// Marshal encodes the packet in binary.
    fn marshal(&self) -> Result<Bytes, Error> {
        Ok(self.0.clone())
    }

    /// Unmarshal decodes the packet from binary.
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error> {
        if raw_packet.len() < HEADER_LENGTH {
            return Err(Error::PacketTooShort);
        }

        let _ = Header::unmarshal(raw_packet)?;

        Ok(RawPacket(raw_packet.clone()))
    }
}

impl RawPacket {
    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        match Header::unmarshal(&self.0) {
            Ok(h) => h,
            Err(_) => Header::default(),
        }
    }
}
