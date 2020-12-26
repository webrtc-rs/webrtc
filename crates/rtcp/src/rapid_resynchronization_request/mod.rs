use super::errors::*;
use super::{header, receiver_report};
use crate::{packet::Packet, util::get_padding};
use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use std::fmt;
use util::Error;

mod rapid_resynchronization_request_test;

const RRR_LENGTH: usize = 2;
const RRR_HEADER_LENGTH: usize = receiver_report::SSRC_LENGTH * 2;
const RRR_MEDIA_OFFSET: usize = 4;

/// The RapidResynchronizationRequest packet informs the encoder about the loss of an undefined amount of coded video data belonging to one or more pictures
#[derive(Debug, PartialEq, Default, Clone)]
pub struct RapidResynchronizationRequest {
    /// SSRC of sender
    pub sender_ssrc: u32,
    /// SSRC of the media source
    pub media_ssrc: u32,
}

impl fmt::Display for RapidResynchronizationRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RapidResynchronizationRequest {:x} {:x}",
            self.sender_ssrc, self.media_ssrc
        )
    }
}

impl Packet for RapidResynchronizationRequest {
    /// Unmarshal decodes the RapidResynchronizationRequest from binary
    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        if raw_packet.len() < (header::HEADER_LENGTH + (receiver_report::SSRC_LENGTH * 2)) {
            return Err(ERR_PACKET_TOO_SHORT.to_owned());
        }

        let mut h = header::Header::default();

        h.unmarshal(raw_packet)?;

        if h.packet_type != header::PacketType::TransportSpecificFeedback
            || h.count != header::FORMAT_RRR
        {
            return Err(ERR_WRONG_TYPE.to_owned());
        }

        self.sender_ssrc = BigEndian::read_u32(&raw_packet[header::HEADER_LENGTH..]);
        self.media_ssrc = BigEndian::read_u32(
            &raw_packet[header::HEADER_LENGTH + receiver_report::SSRC_LENGTH..],
        );

        Ok(())
    }

    /// Marshal encodes the RapidResynchronizationRequest in binary
    fn marshal(&self) -> Result<BytesMut, Error> {
        /*
         * RRR does not require parameters.  Therefore, the length field MUST be
         * 2, and there MUST NOT be any Feedback Control Information.
         *
         * The semantics of this FB message is independent of the payload type.
         */
        let mut raw_packet = BytesMut::new();
        raw_packet.resize(self.len(), 0u8);

        let packet_body = &mut raw_packet[header::HEADER_LENGTH..];

        BigEndian::write_u32(packet_body, self.sender_ssrc);
        BigEndian::write_u32(&mut packet_body[RRR_MEDIA_OFFSET..], self.media_ssrc);

        let header_data = self.header().marshal()?;

        raw_packet[..header_data.len()].copy_from_slice(&header_data);
        Ok(raw_packet)
    }

    /// Destination SSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn trait_eq(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<RapidResynchronizationRequest>()
            .map_or(false, |a| self == a)
    }
}

impl RapidResynchronizationRequest {
    fn len(&self) -> usize {
        header::HEADER_LENGTH + RRR_HEADER_LENGTH
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> header::Header {
        let l = self.len() + get_padding(self.len());

        header::Header {
            padding: get_padding(self.len()) != 0,
            count: header::FORMAT_RRR,
            packet_type: header::PacketType::TransportSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }
}
