#[cfg(test)]
mod rapid_resynchronization_request_test;

use std::any::Any;
use std::fmt;

use bytes::{Buf, BufMut};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;
use crate::header::*;
use crate::packet::*;
use crate::util::*;

type Result<T> = std::result::Result<T, util::Error>;

const RRR_LENGTH: usize = 2;
const RRR_HEADER_LENGTH: usize = SSRC_LENGTH * 2;
const RRR_MEDIA_OFFSET: usize = 4;

/// The RapidResynchronizationRequest packet informs the encoder about the loss of an undefined amount of coded video data belonging to one or more pictures
#[derive(Debug, PartialEq, Eq, Default, Clone)]
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
    /// Header returns the Header associated with this packet.
    fn header(&self) -> Header {
        Header {
            padding: get_padding_size(self.raw_size()) != 0,
            count: FORMAT_RRR,
            packet_type: PacketType::TransportSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }

    /// Destination SSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn raw_size(&self) -> usize {
        HEADER_LENGTH + RRR_HEADER_LENGTH
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<RapidResynchronizationRequest>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for RapidResynchronizationRequest {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for RapidResynchronizationRequest {
    /// Marshal encodes the RapidResynchronizationRequest in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        /*
         * RRR does not require parameters.  Therefore, the length field MUST be
         * 2, and there MUST NOT be any Feedback Control Information.
         *
         * The semantics of this FB message is independent of the payload type.
         */
        if buf.remaining_mut() < self.marshal_size() {
            return Err(Error::BufferTooShort.into());
        }

        let h = self.header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put_u32(self.sender_ssrc);
        buf.put_u32(self.media_ssrc);

        if h.padding {
            put_padding(buf, self.raw_size());
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for RapidResynchronizationRequest {
    /// Unmarshal decodes the RapidResynchronizationRequest from binary
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        let raw_packet_len = raw_packet.remaining();
        if raw_packet_len < (HEADER_LENGTH + (SSRC_LENGTH * 2)) {
            return Err(Error::PacketTooShort.into());
        }

        let h = Header::unmarshal(raw_packet)?;

        if h.packet_type != PacketType::TransportSpecificFeedback || h.count != FORMAT_RRR {
            return Err(Error::WrongType.into());
        }

        let sender_ssrc = raw_packet.get_u32();
        let media_ssrc = raw_packet.get_u32();

        if
        /*h.padding &&*/
        raw_packet.has_remaining() {
            raw_packet.advance(raw_packet.remaining());
        }

        Ok(RapidResynchronizationRequest {
            sender_ssrc,
            media_ssrc,
        })
    }
}
