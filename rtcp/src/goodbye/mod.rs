#[cfg(test)]
mod goodbye_test;

use crate::{error::Error, header::*, packet::*, util::*};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use bytes::{Buf, BufMut, Bytes};
use std::any::Any;
use std::fmt;

type Result<T> = std::result::Result<T, util::Error>;

/// The Goodbye packet indicates that one or more sources are no longer active.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
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

        write!(f, "{out}")
    }
}

impl Packet for Goodbye {
    /// Header returns the Header associated with this packet.
    fn header(&self) -> Header {
        Header {
            padding: get_padding_size(self.raw_size()) != 0,
            count: self.sources.len() as u8,
            packet_type: PacketType::Goodbye,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        self.sources.to_vec()
    }

    fn raw_size(&self) -> usize {
        let srcs_length = self.sources.len() * SSRC_LENGTH;
        let reason_length = self.reason.len() + 1;

        HEADER_LENGTH + srcs_length + reason_length
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<Goodbye>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for Goodbye {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for Goodbye {
    /// marshal_to encodes the packet in binary.
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if self.sources.len() > COUNT_MAX {
            return Err(Error::TooManySources.into());
        }

        if self.reason.len() > SDES_MAX_OCTET_COUNT {
            return Err(Error::ReasonTooLong.into());
        }

        if buf.remaining_mut() < self.marshal_size() {
            return Err(Error::BufferTooShort.into());
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

        let h = self.header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        for source in &self.sources {
            buf.put_u32(*source);
        }

        buf.put_u8(self.reason.len() as u8);
        if !self.reason.is_empty() {
            buf.put(self.reason.clone());
        }

        if h.padding {
            put_padding(buf, self.raw_size());
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for Goodbye {
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
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
        let raw_packet_len = raw_packet.remaining();

        let header = Header::unmarshal(raw_packet)?;
        if header.packet_type != PacketType::Goodbye {
            return Err(Error::WrongType.into());
        }

        if get_padding_size(raw_packet_len) != 0 {
            return Err(Error::PacketTooShort.into());
        }

        let reason_offset = HEADER_LENGTH + header.count as usize * SSRC_LENGTH;

        if reason_offset > raw_packet_len {
            return Err(Error::PacketTooShort.into());
        }

        let mut sources = Vec::with_capacity(header.count as usize);
        for _ in 0..header.count {
            sources.push(raw_packet.get_u32());
        }

        let reason = if reason_offset < raw_packet_len {
            let reason_len = raw_packet.get_u8() as usize;
            let reason_end = reason_offset + 1 + reason_len;

            if reason_end > raw_packet_len {
                return Err(Error::PacketTooShort.into());
            }

            raw_packet.copy_to_bytes(reason_len)
        } else {
            Bytes::new()
        };

        if
        /*header.padding &&*/
        raw_packet.has_remaining() {
            raw_packet.advance(raw_packet.remaining());
        }

        Ok(Goodbye { sources, reason })
    }
}
