use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::BytesMut;
use util::Error;

use super::errors::*;
use super::header::*;
use crate::{packet::Packet, util as utility};

#[cfg(test)]
mod transport_layer_nack_test;

// PacketBitmap shouldn't be used like a normal integral,
// so it's type is masked here. Access it with PacketList().
type PacketBitmap = u16;

// NackPair is a wire-representation of a collection of
// Lost RTP packets
#[derive(Debug, PartialEq, Default, Clone)]
pub struct NackPair {
    // ID of lost packets
    pub packet_id: u16,

    // Bitmask of following lost packets
    pub lost_packets: PacketBitmap,
}

// PacketList returns a list of Nack'd packets that's referenced by a NackPair
impl NackPair {
    pub fn packet_list(&self) -> Vec<u16> {
        let mut out = vec![];
        out.push(self.packet_id);
        let mut b = self.lost_packets;
        let mut i = 0;
        while b != 0 {
            if (b & (1 << i)) != 0 {
                b &= !(1 << i);
                out.push(self.packet_id + i + 1);
            }
            i += 1;
        }
        out
    }
}

const TLN_LENGTH: usize = 2;
const NACK_OFFSET: usize = 8;

// The TransportLayerNack packet informs the encoder about the loss of a transport packet
// IETF RFC 4585, Section 6.2.1
// https://tools.ietf.org/html/rfc4585#section-6.2.1
#[derive(Debug, PartialEq, Default, Clone)]
pub struct TransportLayerNack {
    // SSRC of sender
    pub sender_ssrc: u32,

    // SSRC of the media source
    pub media_ssrc: u32,

    pub nacks: Vec<NackPair>,
}

impl fmt::Display for TransportLayerNack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = format!("TransportLayerNack from {:x}\n", self.sender_ssrc);
        out += format!("\tMedia Ssrc {:x}\n", self.media_ssrc).as_str();
        out += "\tID\tLostPackets\n";
        for nack in &self.nacks {
            out += format!("\t{}\t{:b}\n", nack.packet_id, nack.lost_packets).as_str();
        }
        write!(f, "{}", out)
    }
}

impl Packet for TransportLayerNack {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Unmarshal decodes the ReceptionReport from binary
    fn unmarshal(&self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        todo!()
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    /// Marshal encodes the packet in binary.
    fn marshal(&self) -> Result<BytesMut, Error> {
        todo!()
    }
}

impl TransportLayerNack {
    fn size(&self) -> usize {
        HEADER_LENGTH + NACK_OFFSET + self.nacks.len() * 4
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.size() + utility::get_padding(self.size());
        Header {
            padding: utility::get_padding(self.size()) != 0,
            count: FORMAT_TLN,
            packet_type: PacketType::TransportSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }
}
