#[cfg(test)]
mod slice_loss_indication_test;

use std::any::Any;
use std::fmt;

use bytes::{Buf, BufMut};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;
use crate::header::*;
use crate::packet::*;
use crate::util::*;

type Result<T> = std::result::Result<T, util::Error>;

const SLI_LENGTH: usize = 2;
const SLI_OFFSET: usize = 8;

/// SLIEntry represents a single entry to the SLI packet's
/// list of lost slices.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct SliEntry {
    /// ID of first lost slice
    pub first: u16,
    /// Number of lost slices
    pub number: u16,
    /// ID of related picture
    pub picture: u8,
}

/// The SliceLossIndication packet informs the encoder about the loss of a picture slice
#[derive(Debug, PartialEq, Eq, Default, Clone)]
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
    /// Header returns the Header associated with this packet.
    fn header(&self) -> Header {
        Header {
            padding: get_padding_size(self.raw_size()) != 0,
            count: FORMAT_SLI,
            packet_type: PacketType::TransportSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn raw_size(&self) -> usize {
        HEADER_LENGTH + SLI_OFFSET + self.sli_entries.len() * 4
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<SliceLossIndication>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for SliceLossIndication {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for SliceLossIndication {
    /// Marshal encodes the SliceLossIndication in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if (self.sli_entries.len() + SLI_LENGTH) as u8 > u8::MAX {
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

        for s in &self.sli_entries {
            let sli = ((s.first as u32 & 0x1FFF) << 19)
                | ((s.number as u32 & 0x1FFF) << 6)
                | (s.picture as u32 & 0x3F);

            buf.put_u32(sli);
        }

        if h.padding {
            put_padding(buf, self.raw_size());
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for SliceLossIndication {
    /// Unmarshal decodes the SliceLossIndication from binary
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

        if raw_packet_len < (HEADER_LENGTH + (4 * h.length as usize)) {
            return Err(Error::PacketTooShort.into());
        }

        if h.packet_type != PacketType::TransportSpecificFeedback || h.count != FORMAT_SLI {
            return Err(Error::WrongType.into());
        }

        let sender_ssrc = raw_packet.get_u32();
        let media_ssrc = raw_packet.get_u32();

        let mut i = HEADER_LENGTH + SLI_OFFSET;
        let mut sli_entries = vec![];
        while i < HEADER_LENGTH + h.length as usize * 4 {
            let sli = raw_packet.get_u32();
            sli_entries.push(SliEntry {
                first: ((sli >> 19) & 0x1FFF) as u16,
                number: ((sli >> 6) & 0x1FFF) as u16,
                picture: (sli & 0x3F) as u8,
            });

            i += 4;
        }

        if
        /*h.padding &&*/
        raw_packet.has_remaining() {
            raw_packet.advance(raw_packet.remaining());
        }

        Ok(SliceLossIndication {
            sender_ssrc,
            media_ssrc,
            sli_entries,
        })
    }
}
