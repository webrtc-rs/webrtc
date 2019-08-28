use std::fmt;
use std::io::{Read, Write};

use util::Error;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::errors::*;
use super::header::*;
use super::packet::*;
use crate::get_padding;

#[cfg(test)]
mod goodbye_test;

// The Goodbye packet indicates that one or more sources are no longer active.
#[derive(Debug, PartialEq, Default)]
pub struct Goodbye {
    // The SSRC/CSRC identifiers that are no longer active
    sources: Vec<u32>,
    // Optional text indicating the reason for leaving, e.g., "camera malfunction" or "RTP loop detected"
    reason: String,
}

impl fmt::Display for Goodbye {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut out = format!("Goodbye:\n\tSources:\n");
        for s in &self.sources {
            out += format!("\t{}\n", *s).as_str();
        }
        out += format!("\tReason: {:?}\n", self.reason).as_str();

        write!(f, "{}", out)
    }
}

impl Goodbye {
    fn len(&self) -> usize {
        let srcs_length = self.sources.len() * SSRC_LENGTH;
        let reason_length = self.reason.len() + 1;

        HEADER_LENGTH + srcs_length + reason_length
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.len() + get_padding(self.len());
        Header {
            padding: get_padding(self.len()) != 0,
            count: self.sources.len() as u8,
            packet_type: PacketType::TypeGoodbye,
            length: ((l / 4) - 1) as u16,
        }
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
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

        let header = Header::unmarshal(reader)?;

        if header.packet_type != PacketType::TypeGoodbye {
            return Err(ErrWrongType.clone());
        }
        if get_padding(header.length as usize) != 0 {
            return Err(ErrPacketTooShort.clone());
        }

        let mut sources = vec![];

        for _i in 0..header.count {
            sources.push(reader.read_u32::<BigEndian>()?);
        }

        let mut buf: Vec<u8> = vec![0; 1];
        let num_bytes = reader.read(&mut buf)?;

        let mut reason = String::new();
        if num_bytes == 1 {
            let reason_len = buf[0] as u64;
            let mut reason_reader = reader.take(reason_len);
            reason_reader.read_to_string(&mut reason)?;
            if reason.len() < reason_len as usize {
                return Err(ErrPacketTooShort.clone());
            }
        }

        Ok(Goodbye { sources, reason })
    }
}

impl<W: Write> Packet<W> for Goodbye {
    // DestinationSSRC returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        self.sources.to_vec()
    }

    // Marshal encodes the packet in binary.
    fn marshal(&self, writer: &mut W) -> Result<(), Error> {
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

        if self.sources.len() > COUNT_MAX {
            return Err(ErrTooManySources.clone());
        }

        self.header().marshal(writer)?;

        for s in &self.sources {
            writer.write_u32::<BigEndian>(*s)?;
        }

        if &self.reason != "" {
            if self.reason.len() > SDES_MAX_OCTET_COUNT {
                return Err(ErrReasonTooLong.clone());
            }
            writer.write_u8(self.reason.len() as u8)?;
            writer.write_all(self.reason.as_bytes())?;
        }

        Ok(())
    }
}
