use std::fmt;

use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use header::Header;

use super::{header, receiver_report};
use crate::error::Error;
use crate::packet::Packet;
use crate::util::get_padding;

mod slice_loss_indication_test;

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
    /// Unmarshal decodes the SliceLossIndication from binary
    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        if raw_packet.len() < (header::HEADER_LENGTH + receiver_report::SSRC_LENGTH) {
            return Err(Error::PacketTooShort);
        }

        let mut h = header::Header::default();
        h.unmarshal(raw_packet)?;

        if raw_packet.len() < (header::HEADER_LENGTH + (4 * h.length as usize)) {
            return Err(Error::PacketTooShort);
        }

        if h.packet_type != header::PacketType::TransportSpecificFeedback
            || h.count != header::FORMAT_SLI
        {
            return Err(Error::WrongType);
        }

        self.sender_ssrc = BigEndian::read_u32(&raw_packet[header::HEADER_LENGTH..]);
        self.media_ssrc = BigEndian::read_u32(
            &raw_packet[header::HEADER_LENGTH + receiver_report::SSRC_LENGTH..],
        );

        let mut i = header::HEADER_LENGTH + SLI_OFFSET;

        while i < header::HEADER_LENGTH + h.length as usize * 4 {
            let sli = BigEndian::read_u32(&raw_packet[i..]);

            self.sli_entries.push(SliEntry {
                first: ((sli >> 19) & 0x1FFF) as u16,
                number: ((sli >> 6) & 0x1FFF) as u16,
                picture: (sli & 0x3F) as u8,
            });

            i += 4;
        }

        Ok(())
    }

    /// Marshal encodes the SliceLossIndication in binary
    fn marshal(&self) -> Result<BytesMut, Error> {
        if (self.sli_entries.len() + SLI_LENGTH) as u8 > std::u8::MAX {
            return Err(Error::TooManyReports);
        }

        let mut raw_packet = BytesMut::new();
        raw_packet.resize(SLI_OFFSET + (self.sli_entries.len() * 4), 0u8);

        BigEndian::write_u32(&mut raw_packet, self.sender_ssrc);
        BigEndian::write_u32(&mut raw_packet[4..], self.media_ssrc);

        for (i, s) in self.sli_entries.iter().enumerate() {
            let sli = ((s.first as u32 & 0x1FFF) << 19)
                | ((s.number as u32 & 0x1FFF) << 6)
                | (s.picture as u32 & 0x3F);

            BigEndian::write_u32(&mut raw_packet[SLI_OFFSET + (4 * i)..], sli);
        }

        let mut header_data = self.header().marshal()?;

        header_data.extend(&raw_packet);

        Ok(header_data)
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn trait_eq(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<SliceLossIndication>()
            .map_or(false, |a| self == a)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl SliceLossIndication {
    fn len(&self) -> usize {
        header::HEADER_LENGTH + SLI_OFFSET + self.sli_entries.len() * 4
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.len() + get_padding(self.len());

        Header {
            padding: get_padding(self.len()) != 0,
            count: header::FORMAT_SLI,
            packet_type: header::PacketType::TransportSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }
}
