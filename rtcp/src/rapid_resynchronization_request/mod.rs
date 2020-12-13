use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::BytesMut;

use util::Error;

use super::errors::*;
use super::header::*;
use crate::{packet::Packet, util::get_padding};

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

impl Packet for RapidResynchronizationRequest {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        todo!()
    }

    fn marshal(&self) -> Result<BytesMut, Error> {
        todo!()
    }

    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }
}

impl RapidResynchronizationRequest {
    fn size(&self) -> usize {
        HEADER_LENGTH + RRR_HEADER_LENGTH
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
}
