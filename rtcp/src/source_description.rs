use std::fmt;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use util::Error;

use super::errors::*;
use super::header::*;
use crate::util::get_padding;

#[cfg(test)]
mod source_description_test;

// SDESType is the item type used in the RTCP SDES control packet.
// RTP SDES item types registered with IANA. See: https://www.iana.org/assignments/rtp-parameters/rtp-parameters.xhtml#rtp-parameters-5
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SDESType {
    SDESEnd = 0,      // end of SDES list                RFC 3550, 6.5
    SDESCNAME = 1,    // canonical name                  RFC 3550, 6.5.1
    SDESName = 2,     // user name                       RFC 3550, 6.5.2
    SDESEmail = 3,    // user's electronic mail address  RFC 3550, 6.5.3
    SDESPhone = 4,    // user's phone number             RFC 3550, 6.5.4
    SDESLocation = 5, // geographic user location        RFC 3550, 6.5.5
    SDESTool = 6,     // name of application or tool     RFC 3550, 6.5.6
    SDESNote = 7,     // notice about the source         RFC 3550, 6.5.7
    SDESPrivate = 8,  // private extensions              RFC 3550, 6.5.8  (not implemented)
}

impl Default for SDESType {
    fn default() -> Self {
        SDESType::SDESEnd
    }
}

impl fmt::Display for SDESType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SDESType::SDESEnd => "END",
            SDESType::SDESCNAME => "CNAME",
            SDESType::SDESName => "NAME",
            SDESType::SDESEmail => "EMAIL",
            SDESType::SDESPhone => "PHONE",
            SDESType::SDESLocation => "LOC",
            SDESType::SDESTool => "TOOL",
            SDESType::SDESNote => "NOTE",
            SDESType::SDESPrivate => "PRIV",
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for SDESType {
    fn from(b: u8) -> Self {
        match b {
            1 => SDESType::SDESCNAME,
            2 => SDESType::SDESName,
            3 => SDESType::SDESEmail,
            4 => SDESType::SDESPhone,
            5 => SDESType::SDESLocation,
            6 => SDESType::SDESTool,
            7 => SDESType::SDESNote,
            8 => SDESType::SDESPrivate,
            _ => SDESType::SDESEnd,
        }
    }
}

const SDES_SOURCE_LEN: usize = 4;
const SDES_TYPE_LEN: usize = 1;
const SDES_OCTET_COUNT_LEN: usize = 1;
const SDES_MAX_OCTET_COUNT: usize = (1 << 8) - 1;

// A SourceDescriptionChunk contains items describing a single RTP source
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SourceDescriptionChunk {
    // The source (ssrc) or contributing source (csrc) identifier this packet describes
    pub source: u32,
    pub items: Vec<SourceDescriptionItem>,
}

impl SourceDescriptionChunk {
    // Marshal encodes the SourceDescriptionChunk in binary
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        /*
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         *  |                          SSRC/CSRC_1                          |
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |                           SDES items                          |
         *  |                              ...                              |
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */

        writer.write_u32::<BigEndian>(self.source)?;

        for it in &self.items {
            it.marshal(writer)?;
        }

        // The list of items in each chunk MUST be terminated by one or more null octets
        writer.write_u8(SDESType::SDESEnd as u8)?;

        // additional null octets MUST be included if needed to pad until the next 32-bit boundary
        let padding_len = get_padding(self.size());
        let padding: Vec<u8> = vec![0; padding_len];
        writer.write_all(padding.as_slice())?;

        Ok(writer.flush()?)
    }

    // Unmarshal decodes the SourceDescriptionChunk from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        /*
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         *  |                          SSRC/CSRC_1                          |
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |                           SDES items                          |
         *  |                              ...                              |
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */
        let source = reader.read_u32::<BigEndian>()?;

        let mut items = vec![];
        loop {
            let item = SourceDescriptionItem::unmarshal(reader)?;
            if item.sdes_type != SDESType::SDESEnd {
                items.push(item);
            } else {
                break;
            }
        }

        let sdc = SourceDescriptionChunk { source, items };
        let mut padding_len = get_padding(sdc.size());
        while padding_len > 0 {
            reader.read_u8()?;
            padding_len -= 1;
        }

        Ok(sdc)
    }

    fn size(&self) -> usize {
        let mut len = SDES_SOURCE_LEN;
        for it in &self.items {
            len += it.size();
        }
        len += SDES_TYPE_LEN; // for terminating null octet

        len
    }
}

// A SourceDescriptionItem is a part of a SourceDescription that describes a stream.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SourceDescriptionItem {
    // The type identifier for this item. eg, SDESCNAME for canonical name description.
    //
    // Type zero or SDESEnd is interpreted as the end of an item list and cannot be used.
    pub sdes_type: SDESType,
    // Text is a unicode text blob associated with the item. Its meaning varies based on the item's Type.
    pub text: String,
}

impl SourceDescriptionItem {
    fn size(&self) -> usize {
        /*
         *   0                   1                   2                   3
         *   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |    CNAME=1    |     length    | user and domain name        ...
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */
        SDES_TYPE_LEN + SDES_OCTET_COUNT_LEN + self.text.len()
    }

    // Marshal encodes the SourceDescriptionItem in binary
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        /*
         *   0                   1                   2                   3
         *   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |    CNAME=1    |     length    | user and domain name        ...
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        if self.sdes_type == SDESType::SDESEnd {
            return Err(ERR_SDESMISSING_TYPE.clone());
        }

        writer.write_u8(self.sdes_type as u8)?;

        if self.text.len() > SDES_MAX_OCTET_COUNT {
            return Err(ERR_SDESTEXT_TOO_LONG.clone());
        }

        writer.write_u8(self.text.len() as u8)?;

        writer.write_all(self.text.as_bytes())?;

        Ok(writer.flush()?)
    }

    // Unmarshal decodes the SourceDescriptionItem from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        /*
         *   0                   1                   2                   3
         *   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |    CNAME=1    |     length    | user and domain name        ...
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */
        let b0 = reader.read_u8()?;
        let sdes_type: SDESType = b0.into();
        if sdes_type == SDESType::SDESEnd {
            return Ok(SourceDescriptionItem {
                sdes_type,
                text: String::new(),
            });
        }

        let length = reader.read_u8()?;

        let mut text: Vec<u8> = vec![0; length as usize];
        let result = reader.read_exact(&mut text);
        if result.is_err() {
            Err(ERR_PACKET_TOO_SHORT.clone())
        } else {
            Ok(SourceDescriptionItem {
                sdes_type,
                text: String::from_utf8(text)?,
            })
        }
    }
}

// A SourceDescription (SDES) packet describes the sources in an RTP stream.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SourceDescription {
    pub chunks: Vec<SourceDescriptionChunk>,
}

impl fmt::Display for SourceDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = "Source Description:\n".to_string();
        for c in &self.chunks {
            out += format!("\t{:x}\n", c.source).as_str();
            for it in &c.items {
                out += format!("\t\t{:?}\n", it).as_str();
            }
        }
        write!(f, "{}", out)
    }
}

impl SourceDescription {
    fn size(&self) -> usize {
        let mut chunks_length = 0;
        for c in &self.chunks {
            chunks_length += c.size();
        }
        HEADER_LENGTH + chunks_length
    }

    // Unmarshal decodes the SourceDescription from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        /*
         *         0                   1                   2                   3
         *         0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * header |V=2|P|    SC   |  PT=SDES=202  |             length            |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * chunk  |                          SSRC/CSRC_1                          |
         *   1    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                           SDES items                          |
         *        |                              ...                              |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * chunk  |                          SSRC/CSRC_2                          |
         *   2    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                           SDES items                          |
         *        |                              ...                              |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */

        let header = Header::unmarshal(reader)?;

        if header.packet_type != PacketType::SourceDescription {
            return Err(ERR_WRONG_TYPE.clone());
        }

        let mut chunks = vec![];
        for _i in 0..header.count {
            chunks.push(SourceDescriptionChunk::unmarshal(reader)?);
        }

        Ok(SourceDescription { chunks })
    }

    // Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.size() + get_padding(self.size());
        Header {
            padding: get_padding(self.size()) != 0,
            count: self.chunks.len() as u8,
            packet_type: PacketType::SourceDescription,
            length: ((l / 4) - 1) as u16,
        }
    }

    // destination_ssrc returns an array of SSRC values that this packet refers to.
    pub fn destination_ssrc(&self) -> Vec<u32> {
        self.chunks.iter().map(|x| x.source).collect()
    }

    // Marshal encodes the SourceDescription in binary
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        /*
         *         0                   1                   2                   3
         *         0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *        +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * header |V=2|P|    SC   |  PT=SDES=202  |             length            |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * chunk  |                          SSRC/CSRC_1                          |
         *   1    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                           SDES items                          |
         *        |                              ...                              |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         * chunk  |                          SSRC/CSRC_2                          |
         *   2    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *        |                           SDES items                          |
         *        |                              ...                              |
         *        +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */
        if self.chunks.len() > COUNT_MAX {
            return Err(ERR_TOO_MANY_CHUNKS.clone());
        }

        self.header().marshal(writer)?;

        for c in &self.chunks {
            c.marshal(writer)?;
        }

        Ok(())
    }
}
