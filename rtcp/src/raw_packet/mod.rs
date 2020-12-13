use std::fmt;
use std::io::{BufReader, Read, Write};

use util::Error;

use crate::packet::Packet;
use bytes::BytesMut;

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

impl Packet for RawPacket {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        todo!()
    }

    fn marshal(&self) -> Result<BytesMut, Error> {
        todo!()
    }
}

impl RawPacket {
    fn len(&self) -> usize {
        self.raw.len()
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        self.header.clone()
    }
}
