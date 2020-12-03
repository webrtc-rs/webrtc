use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use util::Error;

use super::errors::*;
use super::header::*;
use crate::util::get_padding;

#[cfg(test)]
mod picture_loss_indication_test;

// The PictureLossIndication packet informs the encoder about the loss of an undefined amount of coded video data belonging to one or more pictures
#[derive(Debug, PartialEq, Default, Clone)]
pub struct PictureLossIndication {
    // SSRC of sender
    pub sender_ssrc: u32,

    // SSRC where the loss was experienced
    pub media_ssrc: u32,
}

const PLI_LENGTH: usize = 2;

impl fmt::Display for PictureLossIndication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PictureLossIndication {:x} {:x}",
            self.sender_ssrc, self.media_ssrc
        )
    }
}

impl PictureLossIndication {
    fn size(&self) -> usize {
        HEADER_LENGTH + SSRC_LENGTH * 2
    }

    // Unmarshal decodes the ReceptionReport from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let header = Header::unmarshal(reader)?;

        if header.packet_type != PacketType::PayloadSpecificFeedback || header.count != FORMAT_PLI {
            return Err(ERR_WRONG_TYPE.clone());
        }

        let sender_ssrc = reader.read_u32::<BigEndian>()?;
        let media_ssrc = reader.read_u32::<BigEndian>()?;

        Ok(PictureLossIndication {
            sender_ssrc,
            media_ssrc,
        })
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.size() + get_padding(self.size());
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_PLI,
            packet_type: PacketType::PayloadSpecificFeedback,
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
