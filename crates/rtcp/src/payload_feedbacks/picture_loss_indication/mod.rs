#[cfg(test)]
mod picture_loss_indication_test;

use crate::{error::Error, header::*, packet::*, util::*};

use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::any::Any;
use std::fmt;

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
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.media_ssrc]
    }

    fn size(&self) -> usize {
        HEADER_LENGTH + SSRC_LENGTH * 2
    }

    /// Marshal encodes the PictureLossIndication in binary
    fn marshal(&self) -> Result<Bytes> {
        /*
         * PLI does not require parameters.  Therefore, the length field MUST be
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

    /// Unmarshal decodes the PictureLossIndication from binary
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        if raw_packet.len() < (HEADER_LENGTH + (SSRC_LENGTH * 2)) {
            return Err(Error::PacketTooShort.into());
        }

        let h = Header::unmarshal(raw_packet)?;

        if h.packet_type != PacketType::PayloadSpecificFeedback || h.count != FORMAT_PLI {
            return Err(Error::WrongType.into());
        }

        let reader = &mut raw_packet.slice(HEADER_LENGTH..);

        let sender_ssrc = reader.get_u32();
        let media_ssrc = reader.get_u32();

        Ok(PictureLossIndication {
            sender_ssrc,
            media_ssrc,
        })
    }

    fn equal(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<PictureLossIndication>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl PictureLossIndication {
    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_PLI,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}
