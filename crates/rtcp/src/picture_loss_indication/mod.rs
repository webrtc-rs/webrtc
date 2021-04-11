use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use std::fmt;

use super::{header, receiver_report};
use crate::util::get_padding;
use crate::{error::Error, packet::Packet};

mod picture_loss_indication_test;

const PLI_LENGTH: usize = 2;

/// The PictureLossIndication packet informs the encoder about the loss of an undefined amount of coded video data belonging to one or more pictures
#[derive(Debug, PartialEq, Default, Clone)]
pub struct PictureLossIndication {
    /// SSRC of sender
    pub sender_ssrc: u32,
    /// SSRC where the loss was experienced
    pub media_ssrc: u32,
}

impl fmt::Display for PictureLossIndication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PictureLossIndication {:x} {:x}",
            self.sender_ssrc, self.media_ssrc
        )
    }
}

impl Packet for PictureLossIndication {
    /// Unmarshal decodes the PictureLossIndication from binary
    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        if raw_packet.len() < (header::HEADER_LENGTH + (receiver_report::SSRC_LENGTH * 2)) {
            return Err(Error::PacketTooShort);
        }

        let mut h = header::Header::default();

        h.unmarshal(raw_packet)?;

        if h.packet_type != header::PacketType::PayloadSpecificFeedback
            || h.count != header::FORMAT_PLI
        {
            return Err(Error::WrongType);
        }

        self.sender_ssrc = BigEndian::read_u32(&raw_packet[header::HEADER_LENGTH..]);
        self.media_ssrc = BigEndian::read_u32(
            &raw_packet[header::HEADER_LENGTH + receiver_report::SSRC_LENGTH..],
        );

        Ok(())
    }

    /// Marshal encodes the PictureLossIndication in binary
    fn marshal(&self) -> Result<BytesMut, Error> {
        /*
         * PLI does not require parameters.  Therefore, the length field MUST be
         * 2, and there MUST NOT be any Feedback Control Information.
         *
         * The semantics of this FB message is independent of the payload type.
         */

        let mut raw_packet = BytesMut::new();
        raw_packet.resize(self.len(), 0u8);

        let mut packet_body = &mut raw_packet[header::HEADER_LENGTH..];

        BigEndian::write_u32(&mut packet_body, self.sender_ssrc);
        BigEndian::write_u32(&mut packet_body[4..], self.media_ssrc);

        let h = header::Header {
            count: header::FORMAT_PLI,
            packet_type: header::PacketType::PayloadSpecificFeedback,
            length: PLI_LENGTH as u16,
            ..Default::default()
        };

        let header_data = h.marshal()?;

        raw_packet[..header_data.len()].copy_from_slice(&header_data);

        Ok(raw_packet)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn trait_eq(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<PictureLossIndication>()
            .map_or(false, |a| self == a)
    }
}

impl PictureLossIndication {
    fn len(&self) -> usize {
        header::HEADER_LENGTH + receiver_report::SSRC_LENGTH * 2
    }

    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> header::Header {
        let l = self.len() + get_padding(self.len());

        header::Header {
            padding: get_padding(self.len()) != 0,
            count: header::FORMAT_PLI,
            packet_type: header::PacketType::PayloadSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }
}
