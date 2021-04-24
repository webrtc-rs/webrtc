#[cfg(test)]
mod transport_layer_nack_test;

use crate::{error::Error, header::*, packet::*, util::*};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::any::Any;
use std::fmt;

/// PacketBitmap shouldn't be used like a normal integral,
/// so it's type is masked here. Access it with PacketList().
type PacketBitmap = u16;

/// NackPair is a wire-representation of a collection of
/// Lost RTP packets
#[derive(Debug, PartialEq, Default, Clone)]
pub struct NackPair {
    /// ID of lost packets
    pub packet_id: u16,
    /// Bitmask of following lost packets
    pub lost_packets: PacketBitmap,
}

impl NackPair {
    /// PacketList returns a list of Nack'd packets that's referenced by a NackPair
    pub fn packet_list(&self) -> Vec<u16> {
        let mut out = vec![self.packet_id];

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
    /// SSRC of sender
    pub sender_ssrc: u32,
    /// SSRC of the media source
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
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn size(&self) -> usize {
        HEADER_LENGTH + NACK_OFFSET + self.nacks.len() * 4
    }

    /// Marshal encodes the packet in binary.
    fn marshal(&self) -> Result<Bytes, Error> {
        if self.nacks.len() + TLN_LENGTH > std::u8::MAX as usize {
            return Err(Error::TooManyReports);
        }

        let mut writer = BytesMut::with_capacity(self.marshal_size());

        let h = self.header();
        let data = h.marshal()?;
        writer.extend(data);

        writer.put_u32(self.sender_ssrc);
        writer.put_u32(self.media_ssrc);

        for i in 0..self.nacks.len() {
            writer.put_u16(self.nacks[i].packet_id);
            writer.put_u16(self.nacks[i].lost_packets);
        }

        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    /// Unmarshal decodes the ReceptionReport from binary
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error> {
        if raw_packet.len() < (HEADER_LENGTH + SSRC_LENGTH) {
            return Err(Error::PacketTooShort);
        }

        let h = Header::unmarshal(raw_packet)?;

        if raw_packet.len() < (HEADER_LENGTH + (4 * h.length) as usize) {
            return Err(Error::PacketTooShort);
        }

        if h.packet_type != PacketType::TransportSpecificFeedback || h.count != FORMAT_TLN {
            return Err(Error::WrongType);
        }

        let reader = &mut raw_packet.slice(HEADER_LENGTH..);

        let sender_ssrc = reader.get_u32();
        let media_ssrc = reader.get_u32();

        let mut nacks = vec![];
        for _i in 0..(h.length as i32 - NACK_OFFSET as i32 / 4) {
            nacks.push(NackPair {
                packet_id: reader.get_u16(),
                lost_packets: reader.get_u16(),
            });
        }

        Ok(TransportLayerNack {
            sender_ssrc,
            media_ssrc,
            nacks,
        })
    }

    fn equal_to(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<TransportLayerNack>()
            .map_or(false, |a| self == a)
    }

    fn clone_to(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl TransportLayerNack {
    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_TLN,
            packet_type: PacketType::TransportSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}

fn nack_pairs_from_sequence_numbers(seq_nos: &[u16]) -> Vec<NackPair> {
    if seq_nos.is_empty() {
        return vec![];
    }

    let mut nack_pair = NackPair {
        packet_id: seq_nos[0],
        ..Default::default()
    };

    let mut pairs = vec![];

    for seq in seq_nos.iter().skip(1) {
        if seq - nack_pair.packet_id > 16 {
            pairs.push(nack_pair.clone());
            nack_pair.packet_id = *seq;
            continue;
        }

        nack_pair.lost_packets |= 1 << (seq - nack_pair.packet_id - 1);
    }

    pairs.push(nack_pair);

    pairs
}
