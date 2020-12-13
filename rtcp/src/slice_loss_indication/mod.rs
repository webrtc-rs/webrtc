use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::BytesMut;
use util::Error;

use super::errors::*;
use super::header::*;
use crate::packet::Packet;
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

impl Packet for SliceLossIndication {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn unmarshal(&self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        todo!()
    }

    fn marshal(&self) -> Result<BytesMut, Error> {
        todo!()
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }
}

impl SliceLossIndication {
    fn size(&self) -> usize {
        HEADER_LENGTH + SLI_OFFSET + self.sli_entries.len() * 4
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
}
