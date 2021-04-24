#[cfg(test)]
mod slice_loss_indication_test;

use crate::{error::Error, header::*, packet::*, util::*};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::any::Any;
use std::fmt;

const SLI_LENGTH: usize = 2;
const SLI_OFFSET: usize = 8;

/// SLIEntry represents a single entry to the SLI packet's
/// list of lost slices.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SliEntry {
    /// ID of first lost slice
    pub first: u16,
    /// Number of lost slices
    pub number: u16,
    /// ID of related picture
    pub picture: u8,
}

/// The SliceLossIndication packet informs the encoder about the loss of a picture slice
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SliceLossIndication {
    /// SSRC of sender
    pub sender_ssrc: u32,
    /// SSRC of the media source
    pub media_ssrc: u32,

    pub sli_entries: Vec<SliEntry>,
}

impl fmt::Display for SliceLossIndication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SliceLossIndication {:x} {:x} {:?}",
            self.sender_ssrc, self.media_ssrc, self.sli_entries,
        )
    }
}

impl Packet for SliceLossIndication {
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn size(&self) -> usize {
        HEADER_LENGTH + SLI_OFFSET + self.sli_entries.len() * 4
    }

    /// Marshal encodes the SliceLossIndication in binary
    fn marshal(&self) -> Result<Bytes, Error> {
        if (self.sli_entries.len() + SLI_LENGTH) as u8 > std::u8::MAX {
            return Err(Error::TooManyReports);
        }

        let mut writer = BytesMut::with_capacity(self.marshal_size());

        let h = self.header();
        let data = h.marshal()?;
        writer.extend(data);

        writer.put_u32(self.sender_ssrc);
        writer.put_u32(self.media_ssrc);

        for s in &self.sli_entries {
            let sli = ((s.first as u32 & 0x1FFF) << 19)
                | ((s.number as u32 & 0x1FFF) << 6)
                | (s.picture as u32 & 0x3F);

            writer.put_u32(sli);
        }

        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    /// Unmarshal decodes the SliceLossIndication from binary
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error> {
        if raw_packet.len() < (HEADER_LENGTH + SSRC_LENGTH) {
            return Err(Error::PacketTooShort);
        }

        let h = Header::unmarshal(raw_packet)?;

        if raw_packet.len() < (HEADER_LENGTH + (4 * h.length as usize)) {
            return Err(Error::PacketTooShort);
        }

        if h.packet_type != PacketType::TransportSpecificFeedback || h.count != FORMAT_SLI {
            return Err(Error::WrongType);
        }

        let reader = &mut raw_packet.slice(HEADER_LENGTH..);

        let sender_ssrc = reader.get_u32();
        let media_ssrc = reader.get_u32();

        let mut i = HEADER_LENGTH + SLI_OFFSET;
        let mut sli_entries = vec![];
        while i < HEADER_LENGTH + h.length as usize * 4 {
            let sli = reader.get_u32();
            sli_entries.push(SliEntry {
                first: ((sli >> 19) & 0x1FFF) as u16,
                number: ((sli >> 6) & 0x1FFF) as u16,
                picture: (sli & 0x3F) as u8,
            });

            i += 4;
        }

        Ok(SliceLossIndication {
            sender_ssrc,
            media_ssrc,
            sli_entries,
        })
    }

    fn equal_to(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<SliceLossIndication>()
            .map_or(false, |a| self == a)
    }

    fn clone_to(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl SliceLossIndication {
    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_SLI,
            packet_type: PacketType::TransportSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}
