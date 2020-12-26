#[cfg(test)]
mod full_intra_request_test;

use crate::errors::*;
use bytes::BytesMut;
use std::fmt;
use util::Error;

use byteorder::{BigEndian, ByteOrder};

use super::{header, header::Header};
use crate::{packet::Packet, util::get_padding};

/// A FIREntry is a (ssrc, seqno) pair, as carried by FullIntraRequest.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct FIREntry {
    ssrc: u32,
    sequence_number: u8,
}

/// The FullIntraRequest packet is used to reliably request an Intra frame
/// in a video stream.  See RFC 5104 Section 3.5.1.  This is not for loss
/// recovery, which should use PictureLossIndication (PLI) instead.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct FullIntraRequest {
    sender_ssrc: u32,
    media_ssrc: u32,

    fir: Vec<FIREntry>,
}

const FIR_OFFSET: usize = 8;

impl fmt::Display for FullIntraRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = format!("FullIntraRequest {} {}", self.sender_ssrc, self.media_ssrc);
        for e in &self.fir {
            out += format!(" ({} {})", e.ssrc, e.sequence_number).as_str();
        }
        write!(f, "{}", out)
    }
}

impl Packet for FullIntraRequest {
    /// Marshal encodes the FullIntraRequest
    fn marshal(&self) -> Result<BytesMut, Error> {
        let mut raw_packet = BytesMut::new();
        raw_packet.resize(FIR_OFFSET + (self.fir.len() * 8), 0u8);

        BigEndian::write_u32(&mut raw_packet, self.sender_ssrc);
        BigEndian::write_u32(&mut raw_packet[4..], self.media_ssrc);

        for (i, fir) in self.fir.iter().enumerate() {
            BigEndian::write_u32(&mut raw_packet[FIR_OFFSET + 8 * i..], fir.ssrc);
            raw_packet[FIR_OFFSET + 8 * i + 4] = fir.sequence_number;
        }

        let header = self.header();

        let mut header_data = header.marshal()?;

        header_data.extend_from_slice(&raw_packet);

        Ok(header_data)
    }

    /// Unmarshal decodes the TransportLayerNack
    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        if raw_packet.len() < (header::HEADER_LENGTH + header::SSRC_LENGTH) {
            return Err(ERR_PACKET_TOO_SHORT.to_owned());
        }

        let mut header = Header::default();

        header.unmarshal(raw_packet)?;

        if raw_packet.len() < (header::HEADER_LENGTH + (4 * header.length) as usize) {
            return Err(ERR_PACKET_TOO_SHORT.to_owned());
        }

        if header.packet_type != header::PacketType::PayloadSpecificFeedback
            || header.count != header::FORMAT_FIR
        {
            return Err(ERR_WRONG_TYPE.to_owned());
        }

        self.sender_ssrc = BigEndian::read_u32(&raw_packet[header::HEADER_LENGTH..]);
        self.media_ssrc =
            BigEndian::read_u32(&raw_packet[header::HEADER_LENGTH + header::SSRC_LENGTH..]);

        let mut i = header::HEADER_LENGTH + FIR_OFFSET;

        while i < header::HEADER_LENGTH + (header.length * 4) as usize {
            self.fir.push(FIREntry {
                ssrc: BigEndian::read_u32(&raw_packet[i..]),
                sequence_number: raw_packet[i + 4],
            });

            i += 8;
        }

        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        let mut ssrcs: Vec<u32> = Vec::with_capacity(self.fir.len());
        for entry in &self.fir {
            ssrcs.push(entry.ssrc);
        }
        ssrcs
    }

    fn trait_eq(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<FullIntraRequest>()
            .map_or(false, |a| self == a)
    }
}

impl FullIntraRequest {
    fn size(&self) -> usize {
        header::HEADER_LENGTH + FIR_OFFSET + self.fir.len() * 8
    }

    pub fn header(&self) -> crate::header::Header {
        let l = self.size() + get_padding(self.size());
        Header {
            padding: get_padding(self.size()) != 0,
            count: header::FORMAT_FIR,
            packet_type: header::PacketType::PayloadSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }
}
