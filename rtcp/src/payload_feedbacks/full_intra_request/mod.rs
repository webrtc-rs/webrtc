#[cfg(test)]
mod full_intra_request_test;

use std::any::Any;
use std::fmt;

use bytes::{Buf, BufMut};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;
use crate::header::*;
use crate::packet::*;
use crate::util::*;

type Result<T> = std::result::Result<T, util::Error>;

/// A FIREntry is a (ssrc, seqno) pair, as carried by FullIntraRequest.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct FirEntry {
    pub ssrc: u32,
    pub sequence_number: u8,
}

/// The FullIntraRequest packet is used to reliably request an Intra frame
/// in a video stream.  See RFC 5104 Section 3.5.1.  This is not for loss
/// recovery, which should use PictureLossIndication (PLI) instead.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct FullIntraRequest {
    pub sender_ssrc: u32,
    pub media_ssrc: u32,
    pub fir: Vec<FirEntry>,
}

const FIR_OFFSET: usize = 8;

impl fmt::Display for FullIntraRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = format!("FullIntraRequest {} {}", self.sender_ssrc, self.media_ssrc);
        for e in &self.fir {
            out += format!(" ({} {})", e.ssrc, e.sequence_number).as_str();
        }
        write!(f, "{out}")
    }
}

impl Packet for FullIntraRequest {
    fn header(&self) -> Header {
        Header {
            padding: get_padding_size(self.raw_size()) != 0,
            count: FORMAT_FIR,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        let mut ssrcs: Vec<u32> = Vec::with_capacity(self.fir.len());
        for entry in &self.fir {
            ssrcs.push(entry.ssrc);
        }
        ssrcs
    }

    fn raw_size(&self) -> usize {
        HEADER_LENGTH + FIR_OFFSET + self.fir.len() * 8
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<FullIntraRequest>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for FullIntraRequest {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for FullIntraRequest {
    /// Marshal encodes the FullIntraRequest
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if buf.remaining_mut() < self.marshal_size() {
            return Err(Error::BufferTooShort.into());
        }

        let h = self.header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put_u32(self.sender_ssrc);
        buf.put_u32(self.media_ssrc);

        for fir in self.fir.iter() {
            buf.put_u32(fir.ssrc);
            buf.put_u8(fir.sequence_number);
            buf.put_u8(0);
            buf.put_u16(0);
        }

        if h.padding {
            put_padding(buf, self.raw_size());
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for FullIntraRequest {
    /// Unmarshal decodes the FullIntraRequest
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
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

        if h.packet_type != PacketType::PayloadSpecificFeedback || h.count != FORMAT_FIR {
            return Err(Error::WrongType.into());
        }

        let sender_ssrc = raw_packet.get_u32();
        let media_ssrc = raw_packet.get_u32();

        let mut i = HEADER_LENGTH + FIR_OFFSET;
        let mut fir = vec![];
        while i < HEADER_LENGTH + (h.length * 4) as usize {
            fir.push(FirEntry {
                ssrc: raw_packet.get_u32(),
                sequence_number: raw_packet.get_u8(),
            });
            raw_packet.get_u8();
            raw_packet.get_u16();

            i += 8;
        }

        if
        /*h.padding &&*/
        raw_packet.has_remaining() {
            raw_packet.advance(raw_packet.remaining());
        }

        Ok(FullIntraRequest {
            sender_ssrc,
            media_ssrc,
            fir,
        })
    }
}
