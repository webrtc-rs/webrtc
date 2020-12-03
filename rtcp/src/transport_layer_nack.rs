use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use util::Error;

use super::errors::*;
use super::header::*;
use crate::util::get_padding;

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

impl TransportLayerNack {
    fn size(&self) -> usize {
        HEADER_LENGTH + NACK_OFFSET + self.nacks.len() * 4
    }

    // Unmarshal decodes the ReceptionReport from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let header = Header::unmarshal(reader)?;

        if header.packet_type != PacketType::TransportSpecificFeedback || header.count != FORMAT_TLN
        {
            return Err(ERR_WRONG_TYPE.clone());
        }

        let sender_ssrc = reader.read_u32::<BigEndian>()?;
        let media_ssrc = reader.read_u32::<BigEndian>()?;

        let mut nacks = vec![];
        for _i in 0..(header.length as i32 - NACK_OFFSET as i32 / 4) {
            nacks.push(NackPair {
                packet_id: reader.read_u16::<BigEndian>()?,
                lost_packets: reader.read_u16::<BigEndian>()?,
            });
        }

        Ok(TransportLayerNack {
            sender_ssrc,
            media_ssrc,
            nacks,
        })
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.size() + get_padding(self.size());
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_TLN,
            packet_type: PacketType::TransportSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    pub fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    // Marshal encodes the packet in binary.
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        if self.nacks.len() + TLN_LENGTH > std::u8::MAX as usize {
            return Err(ERR_TOO_MANY_REPORTS.clone());
        }

        self.header().marshal(writer)?;

        writer.write_u32::<BigEndian>(self.sender_ssrc)?;
        writer.write_u32::<BigEndian>(self.media_ssrc)?;

        for nack in &self.nacks {
            writer.write_u16::<BigEndian>(nack.packet_id)?;
            writer.write_u16::<BigEndian>(nack.lost_packets)?;
        }

        Ok(writer.flush()?)
    }
}
