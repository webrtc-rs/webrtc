#[cfg(test)]
mod rapid_resynchronization_request_test;

use crate::{error::Error, header::*, packet::*, util::*};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::any::Any;
use std::fmt;

const RRR_LENGTH: usize = 2;
const RRR_HEADER_LENGTH: usize = SSRC_LENGTH * 2;
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
    /// Destination SSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn size(&self) -> usize {
        HEADER_LENGTH + RRR_HEADER_LENGTH
    }

    /// Marshal encodes the RapidResynchronizationRequest in binary
    fn marshal(&self) -> Result<Bytes, Error> {
        /*
         * RRR does not require parameters.  Therefore, the length field MUST be
         * 2, and there MUST NOT be any Feedback Control Information.
         *
         * The semantics of this FB message is independent of the payload type.
         */
        let mut writer = BytesMut::with_capacity(self.marshal_size());

        let h = self.header();
        let data = h.marshal()?;
        writer.extend(data);

        writer.put_u32(self.sender_ssrc);
        writer.put_u32(self.media_ssrc);

        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    /// Unmarshal decodes the RapidResynchronizationRequest from binary
    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error> {
        if raw_packet.len() < (HEADER_LENGTH + (SSRC_LENGTH * 2)) {
            return Err(Error::PacketTooShort);
        }

        let h = Header::unmarshal(raw_packet)?;

        if h.packet_type != PacketType::TransportSpecificFeedback || h.count != FORMAT_RRR {
            return Err(Error::WrongType);
        }

        let reader = &mut raw_packet.slice(HEADER_LENGTH..);

        let sender_ssrc = reader.get_u32();
        let media_ssrc = reader.get_u32();

        Ok(RapidResynchronizationRequest {
            sender_ssrc,
            media_ssrc,
        })
    }

    fn equal_to(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<RapidResynchronizationRequest>()
            .map_or(false, |a| self == a)
    }

    fn clone_to(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl RapidResynchronizationRequest {
    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_RRR,
            packet_type: PacketType::TransportSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}
