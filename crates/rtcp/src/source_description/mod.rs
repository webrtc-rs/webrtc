#[cfg(test)]
mod source_description_test;

use crate::{error::Error, header::*, packet::*, util::*};

use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::any::Any;
use std::fmt;

const SDES_SOURCE_LEN: usize = 4;
const SDES_TYPE_LEN: usize = 1;
const SDES_TYPE_OFFSET: usize = 0;
const SDES_OCTET_COUNT_LEN: usize = 1;
const SDES_OCTET_COUNT_OFFSET: usize = 1;
const SDES_MAX_OCTET_COUNT: usize = (1 << 8) - 1;
const SDES_TEXT_OFFSET: usize = 2;

/// SDESType is the item type used in the RTCP SDES control packet.
/// RTP SDES item types registered with IANA. See: https://www.iana.org/assignments/rtp-parameters/rtp-parameters.xhtml#rtp-parameters-5
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum SdesType {
    SdesEnd = 0,      // end of SDES list                RFC 3550, 6.5
    SdesCname = 1,    // canonical name                  RFC 3550, 6.5.1
    SdesName = 2,     // user name                       RFC 3550, 6.5.2
    SdesEmail = 3,    // user's electronic mail address  RFC 3550, 6.5.3
    SdesPhone = 4,    // user's phone number             RFC 3550, 6.5.4
    SdesLocation = 5, // geographic user location        RFC 3550, 6.5.5
    SdesTool = 6,     // name of application or tool     RFC 3550, 6.5.6
    SdesNote = 7,     // notice about the source         RFC 3550, 6.5.7
    SdesPrivate = 8,  // private extensions              RFC 3550, 6.5.8  (not implemented)
}

impl Default for SdesType {
    fn default() -> Self {
        SdesType::SdesEnd
    }
}

impl fmt::Display for SdesType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SdesType::SdesEnd => "END",
            SdesType::SdesCname => "CNAME",
            SdesType::SdesName => "NAME",
            SdesType::SdesEmail => "EMAIL",
            SdesType::SdesPhone => "PHONE",
            SdesType::SdesLocation => "LOC",
            SdesType::SdesTool => "TOOL",
            SdesType::SdesNote => "NOTE",
            SdesType::SdesPrivate => "PRIV",
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for SdesType {
    fn from(b: u8) -> Self {
        match b {
            1 => SdesType::SdesCname,
            2 => SdesType::SdesName,
            3 => SdesType::SdesEmail,
            4 => SdesType::SdesPhone,
            5 => SdesType::SdesLocation,
            6 => SdesType::SdesTool,
            7 => SdesType::SdesNote,
            8 => SdesType::SdesPrivate,
            _ => SdesType::SdesEnd,
        }
    }
}

/// A SourceDescriptionChunk contains items describing a single RTP source
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SourceDescriptionChunk {
    /// The source (ssrc) or contributing source (csrc) identifier this packet describes
    pub source: u32,
    pub items: Vec<SourceDescriptionItem>,
}

impl SourceDescriptionChunk {
    /// Marshal encodes the SourceDescriptionChunk in binary
    pub fn marshal(&self) -> Result<Bytes> {
        /*
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         *  |                          SSRC/CSRC_1                          |
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |                           SDES items                          |
         *  |                              ...                              |
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */

        let mut writer = BytesMut::with_capacity(self.marshal_size());

        writer.put_u32(self.source);

        for it in &self.items {
            let data = it.marshal()?;
            writer.extend(data);
        }

        // The list of items in each chunk MUST be terminated by one or more null octets
        writer.put_u8(SdesType::SdesEnd as u8);

        // additional null octets MUST be included if needed to pad until the next 32-bit boundary
        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    /// Unmarshal decodes the SourceDescriptionChunk from binary
    pub fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        /*
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         *  |                          SSRC/CSRC_1                          |
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |                           SDES items                          |
         *  |                              ...                              |
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */

        if raw_packet.len() < (SDES_SOURCE_LEN + SDES_TYPE_LEN) {
            return Err(Error::PacketTooShort.into());
        }

        let reader = &mut raw_packet.clone();

        let source = reader.get_u32();

        let mut offset = SDES_SOURCE_LEN;
        let mut items = vec![];
        while offset < raw_packet.len() {
            let pkt_type = SdesType::from(reader.get_u8());
            if pkt_type == SdesType::SdesEnd {
                return Ok(SourceDescriptionChunk { source, items });
            }

            let item = SourceDescriptionItem::unmarshal(&raw_packet.slice(offset..))?;
            reader.advance(item.marshal_size() - 1); // reader.get_u8() already consumes one bytes
            offset += item.marshal_size();
            items.push(item);
        }

        Err(Error::PacketTooShort.into())
    }

    pub fn marshal_size(&self) -> usize {
        let mut len = SDES_SOURCE_LEN;
        for it in &self.items {
            len += it.marshal_size();
        }

        len += SDES_TYPE_LEN; // for terminating null octet

        // align to 32-bit boundary
        len + get_padding(len)
    }
}

/// A SourceDescriptionItem is a part of a SourceDescription that describes a stream.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SourceDescriptionItem {
    /// The type identifier for this item. eg, SDESCNAME for canonical name description.
    ///
    /// Type zero or SDESEnd is interpreted as the end of an item list and cannot be used.
    pub sdes_type: SdesType,
    /// Text is a unicode text blob associated with the item. Its meaning varies based on the item's Type.
    pub text: Bytes,
}

impl SourceDescriptionItem {
    pub fn marshal_size(&self) -> usize {
        /*
         *   0                   1                   2                   3
         *   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |    CNAME=1    |     length    | user and domain name        ...
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */
        SDES_TYPE_LEN + SDES_OCTET_COUNT_LEN + self.text.len()
    }

    /// Marshal encodes the SourceDescriptionItem in binary
    pub fn marshal(&self) -> Result<Bytes> {
        /*
         *   0                   1                   2                   3
         *   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |    CNAME=1    |     length    | user and domain name        ...
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        if self.sdes_type == SdesType::SdesEnd {
            return Err(Error::SdesMissingType.into());
        }

        let mut writer = BytesMut::with_capacity(self.marshal_size());

        writer.put_u8(self.sdes_type as u8);

        if self.text.len() > SDES_MAX_OCTET_COUNT {
            return Err(Error::SdesTextTooLong.into());
        }
        writer.put_u8(self.text.len() as u8);
        writer.extend(self.text.clone());

        //no padding for each SourceDescriptionItem
        Ok(writer.freeze())
    }

    /// Unmarshal decodes the SourceDescriptionItem from binary
    pub fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        /*
         *   0                   1                   2                   3
         *   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |    CNAME=1    |     length    | user and domain name        ...
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        if raw_packet.len() < (SDES_TYPE_LEN + SDES_OCTET_COUNT_LEN) {
            return Err(Error::PacketTooShort.into());
        }

        let reader = &mut raw_packet.clone();

        let sdes_type = SdesType::from(reader.get_u8());
        let octet_count = reader.get_u8() as usize;
        if SDES_TEXT_OFFSET + octet_count > raw_packet.len() {
            return Err(Error::PacketTooShort.into());
        }

        let text = raw_packet.slice(SDES_TEXT_OFFSET..SDES_TEXT_OFFSET + octet_count);

        Ok(SourceDescriptionItem { sdes_type, text })
    }
}

/// A SourceDescription (SDES) packet describes the sources in an RTP stream.
#[derive(Debug, Default, PartialEq, Clone)]
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

impl Packet for SourceDescription {
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        self.chunks.iter().map(|x| x.source).collect()
    }

    fn size(&self) -> usize {
        let mut chunks_length = 0;
        for c in &self.chunks {
            chunks_length += c.marshal_size();
        }

        HEADER_LENGTH + chunks_length
    }

    /// Marshal encodes the SourceDescription in binary
    fn marshal(&self) -> Result<Bytes> {
        if self.chunks.len() > COUNT_MAX {
            return Err(Error::TooManyChunks.into());
        }

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

        let mut writer = BytesMut::with_capacity(self.marshal_size());

        let h = self.header();
        let data = h.marshal()?;
        writer.extend(data);

        for c in &self.chunks {
            let data = c.marshal()?;
            writer.extend(data);
        }

        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    /// Unmarshal decodes the SourceDescription from binary
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
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

        let h = Header::unmarshal(raw_packet)?;

        if h.packet_type != PacketType::SourceDescription {
            return Err(Error::WrongType.into());
        }

        let mut offset = HEADER_LENGTH;
        let mut chunks = vec![];
        while offset < raw_packet.len() {
            let chunk = SourceDescriptionChunk::unmarshal(&raw_packet.slice(offset..))?;
            offset += chunk.marshal_size();
            chunks.push(chunk);
        }

        if chunks.len() != h.count as usize {
            return Err(Error::InvalidHeader.into());
        }

        Ok(SourceDescription { chunks })
    }

    fn equal(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<SourceDescription>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl SourceDescription {
    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        Header {
            padding: get_padding(self.size()) != 0,
            count: self.chunks.len() as u8,
            packet_type: PacketType::SourceDescription,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}
