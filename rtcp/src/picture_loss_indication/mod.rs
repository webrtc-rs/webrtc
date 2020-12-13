use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::BytesMut;
use util::Error;

use super::header::*;
use crate::packet::Packet;
use crate::util::get_padding;

#[cfg(test)]
mod picture_loss_indication_test;

// The PictureLossIndication packet informs the encoder about the loss of an undefined amount of coded video data belonging to one or more pictures
#[derive(Debug, PartialEq, Default, Clone)]
pub struct PictureLossIndication {
    // SSRC of sender
    pub sender_ssrc: u32,

    // SSRC where the loss was experienced
    pub media_ssrc: u32,
}

const PLI_LENGTH: usize = 2;

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
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
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

impl PictureLossIndication {
    fn len(&self) -> usize {
        HEADER_LENGTH + SSRC_LENGTH * 2
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.len() + get_padding(self.len());
        Header {
            padding: get_padding(self.len()) != 0,
            count: FORMAT_PLI,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }
}
