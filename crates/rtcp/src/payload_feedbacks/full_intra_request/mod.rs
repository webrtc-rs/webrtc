#[cfg(test)]
mod full_intra_request_test;

use crate::{error::Error, header::*, packet::*, util::*};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

/// A FIREntry is a (ssrc, seqno) pair, as carried by FullIntraRequest.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct FirEntry {
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

    fir: Vec<FirEntry>,
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
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        let mut ssrcs: Vec<u32> = Vec::with_capacity(self.fir.len());
        for entry in &self.fir {
            ssrcs.push(entry.ssrc);
        }
        ssrcs
    }

    fn marshal_size(&self) -> usize {
        let l = self.size();

        // align to 32-bit boundary
        l + get_padding(l)
    }

    /// Marshal encodes the FullIntraRequest
    fn marshal(&self) -> Result<Bytes, Error> {
        let mut writer = BytesMut::with_capacity(self.marshal_size());

        let h = self.header();
        let data = h.marshal()?;
        writer.extend(data);

        writer.put_u32(self.sender_ssrc);
        writer.put_u32(self.media_ssrc);

        for (_, fir) in self.fir.iter().enumerate() {
            writer.put_u32(fir.ssrc);
            writer.put_u8(fir.sequence_number);
            writer.put_u8(0);
            writer.put_u16(0);
        }

        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    /// Unmarshal decodes the FullIntraRequest
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error> {
        if raw_packet.len() < (HEADER_LENGTH + SSRC_LENGTH) {
            return Err(Error::PacketTooShort);
        }

        let h = Header::unmarshal(raw_packet)?;

        if raw_packet.len() < (HEADER_LENGTH + (4 * h.length) as usize) {
            return Err(Error::PacketTooShort);
        }

        if h.packet_type != PacketType::PayloadSpecificFeedback || h.count != FORMAT_FIR {
            return Err(Error::WrongType);
        }

        let reader = &mut raw_packet.slice(HEADER_LENGTH..);

        let sender_ssrc = reader.get_u32();
        let media_ssrc = reader.get_u32();

        let mut i = HEADER_LENGTH + FIR_OFFSET;
        let mut fir = vec![];
        while i < HEADER_LENGTH + (h.length * 4) as usize {
            fir.push(FirEntry {
                ssrc: reader.get_u32(),
                sequence_number: reader.get_u8(),
            });
            reader.get_u8();
            reader.get_u16();

            i += 8;
        }

        Ok(FullIntraRequest {
            sender_ssrc,
            media_ssrc,
            fir,
        })
    }

    fn equal_to(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<FullIntraRequest>()
            .map_or(false, |a| self == a)
    }

    fn clone_to(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl FullIntraRequest {
    fn size(&self) -> usize {
        HEADER_LENGTH + FIR_OFFSET + self.fir.len() * 8
    }

    pub fn header(&self) -> Header {
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_FIR,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}
