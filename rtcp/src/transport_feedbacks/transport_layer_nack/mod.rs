#[cfg(test)]
mod transport_layer_nack_test;

use std::any::Any;
use std::fmt;

use bytes::{Buf, BufMut};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;
use crate::header::*;
use crate::packet::*;
use crate::util::*;

/// PacketBitmap shouldn't be used like a normal integral,
/// so it's type is masked here. Access it with PacketList().
type PacketBitmap = u16;

/// NackPair is a wire-representation of a collection of
/// Lost RTP packets
#[derive(Debug, PartialEq, Eq, Default, Clone, Copy)]
pub struct NackPair {
    /// ID of lost packets
    pub packet_id: u16,
    /// Bitmask of following lost packets
    pub lost_packets: PacketBitmap,
}

pub struct NackIterator {
    packet_id: u16,
    bitfield: PacketBitmap,
    has_yielded_packet_id: bool,
}

impl Iterator for NackIterator {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.has_yielded_packet_id {
            self.has_yielded_packet_id = true;

            Some(self.packet_id)
        } else {
            let mut i = 0;

            while self.bitfield != 0 {
                if (self.bitfield & (1 << i)) != 0 {
                    self.bitfield &= !(1 << i);

                    return Some(self.packet_id.wrapping_add(i + 1));
                }

                i += 1;
            }

            None
        }
    }
}

impl NackPair {
    pub fn new(seq: u16) -> Self {
        Self {
            packet_id: seq,
            lost_packets: Default::default(),
        }
    }

    /// PacketList returns a list of Nack'd packets that's referenced by a NackPair
    pub fn packet_list(&self) -> Vec<u16> {
        self.into_iter().collect()
    }

    pub fn range<F>(&self, f: F)
    where
        F: Fn(u16) -> bool,
    {
        for packet_id in self.into_iter() {
            if !f(packet_id) {
                return;
            }
        }
    }
}

/// Create an iterator over all the packet sequence numbers expressed by this NACK pair.
impl IntoIterator for NackPair {
    type Item = u16;

    type IntoIter = NackIterator;

    fn into_iter(self) -> Self::IntoIter {
        NackIterator {
            packet_id: self.packet_id,
            bitfield: self.lost_packets,
            has_yielded_packet_id: false,
        }
    }
}

const TLN_LENGTH: usize = 2;
const NACK_OFFSET: usize = 8;

// The TransportLayerNack packet informs the encoder about the loss of a transport packet
/// ## Specifications
///
/// * [RFC 4585 §6.2.1]
///
/// [RFC 4585 §6.2.1]: https://tools.ietf.org/html/rfc4585#section-6.2.1
#[derive(Debug, PartialEq, Eq, Default, Clone)]
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
        write!(f, "{out}")
    }
}

impl Packet for TransportLayerNack {
    /// returns the Header associated with this packet.
    fn header(&self) -> Header {
        Header {
            padding: get_padding_size(self.raw_size()) != 0,
            count: FORMAT_TLN,
            packet_type: PacketType::TransportSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn raw_size(&self) -> usize {
        HEADER_LENGTH + NACK_OFFSET + self.nacks.len() * 4
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<TransportLayerNack>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for TransportLayerNack {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for TransportLayerNack {
    /// Marshal encodes the packet in binary.
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, util::Error> {
        if self.nacks.len() + TLN_LENGTH > u8::MAX as usize {
            return Err(Error::TooManyReports.into());
        }
        if buf.remaining_mut() < self.marshal_size() {
            return Err(Error::BufferTooShort.into());
        }

        let h = self.header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put_u32(self.sender_ssrc);
        buf.put_u32(self.media_ssrc);

        for i in 0..self.nacks.len() {
            buf.put_u16(self.nacks[i].packet_id);
            buf.put_u16(self.nacks[i].lost_packets);
        }

        if h.padding {
            put_padding(buf, self.raw_size());
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for TransportLayerNack {
    /// Unmarshal decodes the ReceptionReport from binary
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self, util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        let raw_packet_len = raw_packet.remaining();
        if raw_packet_len < (HEADER_LENGTH + SSRC_LENGTH) {
            return Err(Error::PacketTooShort.into());
        }

        let h = Header::unmarshal(raw_packet)?;

        if raw_packet_len < (HEADER_LENGTH + (4 * h.length) as usize) {
            return Err(Error::PacketTooShort.into());
        }

        if h.packet_type != PacketType::TransportSpecificFeedback || h.count != FORMAT_TLN {
            return Err(Error::WrongType.into());
        }

        let sender_ssrc = raw_packet.get_u32();
        let media_ssrc = raw_packet.get_u32();

        let mut nacks = vec![];
        for _i in 0..(h.length as i32 - NACK_OFFSET as i32 / 4) {
            nacks.push(NackPair {
                packet_id: raw_packet.get_u16(),
                lost_packets: raw_packet.get_u16(),
            });
        }

        if
        /*h.padding &&*/
        raw_packet.has_remaining() {
            raw_packet.advance(raw_packet.remaining());
        }

        Ok(TransportLayerNack {
            sender_ssrc,
            media_ssrc,
            nacks,
        })
    }
}

pub fn nack_pairs_from_sequence_numbers(seq_nos: &[u16]) -> Vec<NackPair> {
    if seq_nos.is_empty() {
        return vec![];
    }

    let mut nack_pair = NackPair::new(seq_nos[0]);
    let mut pairs = vec![];

    for &seq in seq_nos.iter().skip(1) {
        if seq == nack_pair.packet_id {
            continue;
        }
        if seq <= nack_pair.packet_id || seq > nack_pair.packet_id.saturating_add(16) {
            pairs.push(nack_pair);
            nack_pair = NackPair::new(seq);
            continue;
        }

        // Subtraction here is safe because the above checks that seqnum > nack_pair.packet_id.
        nack_pair.lost_packets |= 1 << (seq - nack_pair.packet_id - 1);
    }

    pairs.push(nack_pair);

    pairs
}
