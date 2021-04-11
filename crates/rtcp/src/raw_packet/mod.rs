use std::fmt;

use bytes::BytesMut;
use header::Header;

use crate::{error::Error, header, packet::Packet};

mod raw_packet_test;

/// RawPacket represents an unparsed RTCP packet. It's returned by Unmarshal when
/// a packet with an unknown type is encountered.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct RawPacket(Vec<u8>);

impl fmt::Display for RawPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawPacket: {:?}", self)
    }
}

impl Packet for RawPacket {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    /// Unmarshal decodes the packet from binary.
    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        if raw_packet.len() < header::HEADER_LENGTH {
            return Err(Error::PacketTooShort);
        }

        *self = Self(raw_packet.to_vec());

        let mut h = Header::default();
        h.unmarshal(raw_packet)
    }

    /// Marshal encodes the packet in binary.
    fn marshal(&self) -> Result<BytesMut, Error> {
        Ok(self.0[..].into())
    }

    fn trait_eq(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<RawPacket>()
            .map_or(false, |a| self == a)
    }
}

impl RawPacket {
    /// Header returns the Header associated with this packet.
    pub fn header(&mut self) -> header::Header {
        let mut h = header::Header::default();

        match h.unmarshal(&mut self.0.as_slice().into()) {
            Ok(_) => h,

            // ToDo: @metaclips: log error.
            Err(_) => header::Header::default(),
        }
    }
}
