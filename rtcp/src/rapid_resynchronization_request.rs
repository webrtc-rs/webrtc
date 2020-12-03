use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use util::Error;

use super::errors::*;
use super::header::*;
use crate::util::get_padding;

#[cfg(test)]
mod rapid_resynchronization_request_test;

// The RapidResynchronizationRequest packet informs the encoder about the loss of an undefined amount of coded video data belonging to one or more pictures
#[derive(Debug, PartialEq, Default, Clone)]
pub struct RapidResynchronizationRequest {
    // SSRC of sender
    pub sender_ssrc: u32,

    // SSRC of the media source
    pub media_ssrc: u32,
}

const RRR_LENGTH: usize = 2;
const RRR_HEADER_LENGTH: usize = SSRC_LENGTH * 2;
const RRR_MEDIA_OFFSET: usize = 4;

impl fmt::Display for RapidResynchronizationRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RapidResynchronizationRequest {:x} {:x}",
            self.sender_ssrc, self.media_ssrc
        )
    }
}

impl RapidResynchronizationRequest {
    fn size(&self) -> usize {
        HEADER_LENGTH + RRR_HEADER_LENGTH
    }

    // Unmarshal decodes the ReceptionReport from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let header = Header::unmarshal(reader)?;

        if header.packet_type != PacketType::TransportSpecificFeedback || header.count != FORMAT_RRR
        {
            return Err(ERR_WRONG_TYPE.clone());
        }

        let sender_ssrc = reader.read_u32::<BigEndian>()?;
        let media_ssrc = reader.read_u32::<BigEndian>()?;

        Ok(RapidResynchronizationRequest {
            sender_ssrc,
            media_ssrc,
        })
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.size() + get_padding(self.size());
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_RRR,
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
        self.header().marshal(writer)?;

        writer.write_u32::<BigEndian>(self.sender_ssrc)?;
        writer.write_u32::<BigEndian>(self.media_ssrc)?;

        Ok(writer.flush()?)
    }
}
