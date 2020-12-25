#[cfg(test)]
mod full_intra_request_test;

use std::fmt;
use std::io::{Read, Write};

use util::Error;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::errors::*;
use super::header::*;
use crate::util::get_padding;

// A FIREntry is a (ssrc, seqno) pair, as carried by FullIntraRequest.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct FIREntry {
    ssrc: u32,
    sequence_number: u8,
}

// The FullIntraRequest packet is used to reliably request an Intra frame
// in a video stream.  See RFC 5104 Section 3.5.1.  This is not for loss
// recovery, which should use PictureLossIndication (PLI) instead.
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

impl FullIntraRequest {
    fn size(&self) -> usize {
        HEADER_LENGTH + FIR_OFFSET + self.fir.len() * 8
    }

    pub fn header(&self) -> Header {
        let l = self.size() + get_padding(self.size());
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_FIR,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }

    pub fn destination_ssrc(&self) -> Vec<u32> {
        let mut ssrcs: Vec<u32> = Vec::with_capacity(self.fir.len());
        for entry in &self.fir {
            ssrcs.push(entry.ssrc);
        }
        ssrcs
    }

    // Marshal encodes the FullIntraRequest
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        let header = self.header();
        header.marshal(writer)?;

        writer.write_u32::<BigEndian>(self.sender_ssrc)?;
        writer.write_u32::<BigEndian>(self.media_ssrc)?;

        for fir in &self.fir {
            writer.write_u32::<BigEndian>(fir.ssrc)?;
            writer.write_u8(fir.sequence_number)?;
            writer.write_u8(0)?;
            writer.write_u16::<BigEndian>(0)?;
        }

        Ok(writer.flush()?)
    }

    // Unmarshal decodes the TransportLayerNack
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let header = Header::unmarshal(reader)?;

        if header.packet_type != PacketType::PayloadSpecificFeedback || header.count != FORMAT_FIR {
            return Err(ERR_WRONG_TYPE.clone());
        }

        let sender_ssrc = reader.read_u32::<BigEndian>()?;
        let media_ssrc = reader.read_u32::<BigEndian>()?;
        let mut fir: Vec<FIREntry> = vec![];
        for _ in (FIR_OFFSET..header.length as usize * 4).step_by(8) {
            let ssrc = reader.read_u32::<BigEndian>()?;
            let sequence_number = reader.read_u8()?;
            reader.read_u8()?;
            reader.read_u16::<BigEndian>()?;
            fir.push(FIREntry {
                ssrc,
                sequence_number,
            });
        }

        Ok(FullIntraRequest {
            sender_ssrc,
            media_ssrc,

            fir,
        })
    }
}
