use std::fmt;
use std::io::{Read, Write};

use util::Error;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::errors::*;
use super::header::*;
use crate::util::get_padding;

#[cfg(test)]
mod goodbye_test;

// The Goodbye packet indicates that one or more sources are no longer active.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Goodbye {
    // The SSRC/CSRC identifiers that are no longer active
    pub sources: Vec<u32>,
    // Optional text indicating the reason for leaving, e.g., "camera malfunction" or "RTP loop detected"
    pub reason: String,
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

impl Goodbye {
    fn size(&self) -> usize {
        let srcs_length = self.sources.len() * SSRC_LENGTH;
        let reason_length = if self.reason.is_empty() {
            0
        } else {
            self.reason.len() + 1
        };

        HEADER_LENGTH + srcs_length + reason_length
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

        if header.packet_type != PacketType::Goodbye {
            return Err(ERR_WRONG_TYPE.clone());
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
                return Err(ERR_PACKET_TOO_SHORT.clone());
            }
        }

        let goodbye = Goodbye { sources, reason };
        let mut padding_len = get_padding(goodbye.size());
        while padding_len > 0 {
            reader.read_u8()?;
            padding_len -= 1;
        }

        Ok(goodbye)
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.size() + get_padding(self.size());
        Header {
            padding: get_padding(self.size()) != 0,
            count: self.sources.len() as u8,
            packet_type: PacketType::Goodbye,
            length: ((l / 4) - 1) as u16,
        }
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    pub fn destination_ssrc(&self) -> Vec<u32> {
        self.sources.to_vec()
    }

    // Marshal encodes the packet in binary.
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
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
            return Err(ERR_TOO_MANY_SOURCES.clone());
        }

        let header = self.header();
        header.marshal(writer)?;

        for s in &self.sources {
            writer.write_u32::<BigEndian>(*s)?;
        }

        if &self.reason != "" {
            if self.reason.len() > SDES_MAX_OCTET_COUNT {
                return Err(ERR_REASON_TOO_LONG.clone());
            }
            writer.write_u8(self.reason.len() as u8)?;
            writer.write_all(self.reason.as_bytes())?;
        }

        Ok(writer.flush()?)
    }
}
