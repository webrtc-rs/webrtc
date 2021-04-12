#[cfg(test)]
mod goodbye_test;

use crate::{error::Error, header::*, packet::*, util::*};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

/// The Goodbye packet indicates that one or more sources are no longer active.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Goodbye {
    /// The SSRC/CSRC identifiers that are no longer active
    pub sources: Vec<u32>,
    /// Optional text indicating the reason for leaving, e.g., "camera malfunction" or "RTP loop detected"
    pub reason: Bytes,
}

impl fmt::Display for Goodbye {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = "Goodbye:\n\tSources:\n".to_string();
        for s in &self.sources {
            out += format!("\t{}\n", *s).as_str();
        }
        out += format!("\tReason: {:?}\n", self.reason).as_str();

        write!(f, "{}", out)
    }
}

impl Packet for Goodbye {
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        self.sources.to_vec()
    }

    fn marshal_size(&self) -> usize {
        let srcs_length = self.sources.len() * SSRC_LENGTH;
        let reason_length = self.reason.len() + 1;

        let l = HEADER_LENGTH + srcs_length + reason_length;

        // align to 32-bit boundary
        l + get_padding(l)
    }

    /// Marshal encodes the packet in binary.
    fn marshal(&self) -> Result<Bytes, Error> {
        if self.sources.len() > COUNT_MAX {
            return Err(Error::TooManySources);
        }

        if self.reason.len() > SDES_MAX_OCTET_COUNT {
            return Err(Error::ReasonTooLong);
        }

        /*
         *        0                   1                   2                   3
         *        0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *       +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *       |V=2|P|    SC   |   PT=BYE=203  |             length            |
         *       +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *       |                           SSRC/CSRC                           |
         *       +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *       :                              ...                              :
         *       +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * (opt) |     length    |               reason for leaving            ...
         *       +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */
        let mut writer = BytesMut::with_capacity(self.marshal_size());

        let h = self.header();
        let data = h.marshal()?;
        writer.extend(data);

        for source in &self.sources {
            writer.put_u32(*source);
        }

        if !self.reason.is_empty() {
            writer.put_u8(self.reason.len() as u8);
            writer.extend(self.reason.clone());
        }

        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    fn unmarshal(raw_packet: &Bytes) -> Result<Self, Error> {
        /*
         *        0                   1                   2                   3
         *        0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *       +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *       |V=2|P|    SC   |   PT=BYE=203  |             length            |
         *       +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *       |                           SSRC/CSRC                           |
         *       +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *       :                              ...                              :
         *       +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * (opt) |     length    |               reason for leaving            ...
         *       +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        let header = Header::unmarshal(raw_packet)?;

        if header.packet_type != PacketType::Goodbye {
            return Err(Error::WrongType);
        }

        if get_padding(raw_packet.len()) != 0 {
            return Err(Error::PacketTooShort);
        }

        let reason_offset = (HEADER_LENGTH + header.count as usize * SSRC_LENGTH) as usize;

        if reason_offset > raw_packet.len() {
            return Err(Error::PacketTooShort);
        }

        let reader = &mut raw_packet.slice(HEADER_LENGTH..);

        let mut sources = Vec::with_capacity(header.count as usize);
        for _ in 0..header.count {
            sources.push(reader.get_u32());
        }

        let reason = if reason_offset < raw_packet.len() {
            let reason_len = reader.get_u8() as usize;
            let reason_end = reason_offset + 1 + reason_len;

            if reason_end > raw_packet.len() {
                return Err(Error::PacketTooShort);
            }

            raw_packet.slice(reason_offset + 1..reason_end)
        } else {
            Bytes::new()
        };

        Ok(Goodbye { sources, reason })
    }

    /*fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn equal(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<Goodbye>()
            .map_or(false, |a| self == a)
    }*/
}

impl Goodbye {
    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        Header {
            padding: false,
            count: self.sources.len() as u8,
            packet_type: PacketType::Goodbye,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}
