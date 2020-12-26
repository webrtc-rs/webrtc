use std::fmt;

use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use util::Error;

use super::errors::*;
use super::header;
use crate::{packet::Packet, receiver_report, util as utility};

mod transport_layer_nack_test;

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
    /// Marshal encodes the packet in binary.
    fn marshal(&self) -> Result<BytesMut, Error> {
        if self.nacks.len() + TLN_LENGTH > std::u8::MAX as usize {
            return Err(ERR_TOO_MANY_REPORTS.to_owned());
        }

        let mut raw_packet = BytesMut::new();
        raw_packet.resize(NACK_OFFSET + (self.nacks.len() * 4), 0u8);

        BigEndian::write_u32(&mut raw_packet, self.sender_ssrc);
        BigEndian::write_u32(&mut raw_packet[4..], self.media_ssrc);

        for i in 0..self.nacks.len() {
            BigEndian::write_u16(
                &mut raw_packet[NACK_OFFSET + (4 * i)..],
                self.nacks[i].packet_id,
            );

            BigEndian::write_u16(
                &mut raw_packet[NACK_OFFSET + (4 * i) + 2..],
                self.nacks[i].lost_packets,
            );
        }

        let h = self.header();

        let mut header_data = h.marshal()?;
        header_data.extend(&raw_packet);

        Ok(header_data)
    }

    /// Unmarshal decodes the ReceptionReport from binary
    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        if raw_packet.len() < (header::HEADER_LENGTH + receiver_report::SSRC_LENGTH) {
            return Err(ERR_PACKET_TOO_SHORT.to_owned());
        }

        let mut h = header::Header::default();

        h.unmarshal(raw_packet)?;

        if raw_packet.len() < (header::HEADER_LENGTH + (4 * h.length) as usize) {
            return Err(ERR_PACKET_TOO_SHORT.to_owned());
        }

        if h.packet_type != header::PacketType::TransportSpecificFeedback
            || h.count != header::FORMAT_TLN
        {
            return Err(ERR_WRONG_TYPE.to_owned());
        }

        self.sender_ssrc = BigEndian::read_u32(&raw_packet[header::HEADER_LENGTH..]);
        self.media_ssrc = BigEndian::read_u32(
            &raw_packet[header::HEADER_LENGTH + receiver_report::SSRC_LENGTH..],
        );

        let mut i = header::HEADER_LENGTH + NACK_OFFSET;

        while i < (header::HEADER_LENGTH + (h.length * 4) as usize) {
            self.nacks.push(NackPair {
                packet_id: BigEndian::read_u16(&raw_packet[i..]),
                lost_packets: PacketBitmap::from(BigEndian::read_u16(&raw_packet[i + 2..])),
            });

            i += 4
        }

        Ok(())
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn trait_eq(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<TransportLayerNack>()
            .map_or(false, |a| self == a)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl TransportLayerNack {
    fn size(&self) -> usize {
        header::HEADER_LENGTH + NACK_OFFSET + self.nacks.len() * 4
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> header::Header {
        let l = self.size() + utility::get_padding(self.size());

        header::Header {
            padding: utility::get_padding(self.size()) != 0,
            count: header::FORMAT_TLN,
            packet_type: header::PacketType::TransportSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }
}

fn nack_pairs_from_sequence_numbers(seq_nos: &[u16]) -> Vec<NackPair> {
    if seq_nos.len() == 0 {
        return vec![];
    }

    let mut nack_pair = NackPair {
        packet_id: seq_nos[0],
        ..Default::default()
    };

    let mut pairs = vec![];

    for i in 1..seq_nos.len() {
        let m = seq_nos[i];

        if m - nack_pair.packet_id > 16 {
            pairs.push(nack_pair.clone());
            nack_pair.packet_id = m;
            continue;
        }

        nack_pair.lost_packets |= 1 << (m - nack_pair.packet_id - 1);
    }

    pairs.push(nack_pair);

    pairs
}
