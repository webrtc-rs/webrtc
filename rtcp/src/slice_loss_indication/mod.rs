use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use util::Error;

use super::errors::*;
use super::header::*;
use crate::util::get_padding;

#[cfg(test)]
mod slice_loss_indication_test;

// SLIEntry represents a single entry to the SLI packet's
// list of lost slices.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SLIEntry {
    // ID of first lost slice
    pub first: u16,

    // Number of lost slices
    pub number: u16,

    // ID of related picture
    pub picture: u8,
}

// The SliceLossIndication packet informs the encoder about the loss of a picture slice
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SliceLossIndication {
    // SSRC of sender
    pub sender_ssrc: u32,

    // SSRC of the media source
    pub media_ssrc: u32,

    pub sli_entries: Vec<SLIEntry>,
}

const SLI_LENGTH: usize = 2;
const SLI_OFFSET: usize = 8;

impl fmt::Display for SliceLossIndication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SliceLossIndication {:x} {:x} {:?}",
            self.sender_ssrc, self.media_ssrc, self.sli_entries,
        )
    }
}

impl SliceLossIndication {
    fn size(&self) -> usize {
        HEADER_LENGTH + SLI_OFFSET + self.sli_entries.len() * 4
    }

    // Unmarshal decodes the ReceptionReport from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let header = Header::unmarshal(reader)?;

        if header.packet_type != PacketType::TransportSpecificFeedback || header.count != FORMAT_SLI
        {
            return Err(ERR_WRONG_TYPE.clone());
        }

        let sender_ssrc = reader.read_u32::<BigEndian>()?;
        let media_ssrc = reader.read_u32::<BigEndian>()?;

        let mut sli_entries = vec![];
        for _i in 0..(header.length as i32 - SLI_OFFSET as i32 / 4) {
            let sli_entry = reader.read_u32::<BigEndian>()?;
            sli_entries.push(SLIEntry {
                first: ((sli_entry >> 19) & 0x1FFF) as u16,
                number: ((sli_entry >> 6) & 0x1FFF) as u16,
                picture: (sli_entry & 0x3F) as u8,
            });
        }

        Ok(SliceLossIndication {
            sender_ssrc,
            media_ssrc,
            sli_entries,
        })
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.size() + get_padding(self.size());
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_SLI,
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
        if self.sli_entries.len() + SLI_LENGTH > std::u8::MAX as usize {
            return Err(ERR_TOO_MANY_REPORTS.clone());
        }

        self.header().marshal(writer)?;

        writer.write_u32::<BigEndian>(self.sender_ssrc)?;
        writer.write_u32::<BigEndian>(self.media_ssrc)?;

        for sli_entry in &self.sli_entries {
            let sli = ((sli_entry.first & 0x1FFF) as u32) << 19
                | ((sli_entry.number & 0x1FFF) as u32) << 6
                | (sli_entry.picture & 0x3F) as u32;
            writer.write_u32::<BigEndian>(sli)?;
        }

        Ok(writer.flush()?)
    }
}
