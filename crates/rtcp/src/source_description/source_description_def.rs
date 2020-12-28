use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use std::fmt;

use crate::errors::Error;
use crate::header;
use crate::util::get_padding;
use crate::{packet::Packet, source_description};

/// A SourceDescriptionChunk contains items describing a single RTP source
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SourceDescriptionChunk {
    /// The source (ssrc) or contributing source (csrc) identifier this packet describes
    pub source: u32,
    pub items: Vec<SourceDescriptionItem>,
}

impl SourceDescriptionChunk {
    /// Marshal encodes the SourceDescriptionChunk in binary
    pub fn marshal(&self) -> Result<BytesMut, Error> {
        /*
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         *  |                          SSRC/CSRC_1                          |
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |                           SDES items                          |
         *  |                              ...                              |
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */

        let mut raw_packet = BytesMut::new();
        raw_packet.resize(super::SDES_SOURCE_LEN, 0u8);

        BigEndian::write_u32(&mut raw_packet, self.source);

        for it in &self.items {
            let data = it.marshal()?;

            raw_packet.extend(data);
        }

        // The list of items in each chunk MUST be terminated by one or more null octets
        raw_packet.extend(&[super::SDESType::SDESEnd as u8]);

        // additional null octets MUST be included if needed to pad until the next 32-bit boundary
        raw_packet.extend(vec![0u8; get_padding(raw_packet.len())]);

        Ok(raw_packet)
    }

    /// Unmarshal decodes the SourceDescriptionChunk from binary
    pub fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        /*
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         *  |                          SSRC/CSRC_1                          |
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |                           SDES items                          |
         *  |                              ...                              |
         *  +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
         */

        if raw_packet.len() < (super::SDES_SOURCE_LEN + super::SDES_TYPE_LEN) {
            return Err(Error::PacketTooShort);
        }

        self.source = BigEndian::read_u32(raw_packet);

        let mut i = 4;

        while i < raw_packet.len() {
            let pkt_type = super::SDESType::from(raw_packet[i]);

            if pkt_type == super::SDESType::SDESEnd {
                return Ok(());
            }

            let mut it = SourceDescriptionItem::default();

            it.unmarshal(&mut raw_packet[i..].into())?;

            i += it.len();
            self.items.push(it);
        }

        Err(Error::PacketTooShort)
    }

    fn len(&self) -> usize {
        let mut len = super::SDES_SOURCE_LEN;
        for it in &self.items {
            len += it.len();
        }

        len += super::SDES_TYPE_LEN; // for terminating null octet

        // align to 32-bit boundary
        len += get_padding(len);

        len
    }
}

/// A SourceDescriptionItem is a part of a SourceDescription that describes a stream.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SourceDescriptionItem {
    /// The type identifier for this item. eg, SDESCNAME for canonical name description.
    ///
    /// Type zero or SDESEnd is interpreted as the end of an item list and cannot be used.
    pub sdes_type: super::SDESType,
    /// Text is a unicode text blob associated with the item. Its meaning varies based on the item's Type.
    pub text: String,
}

impl SourceDescriptionItem {
    fn len(&self) -> usize {
        /*
         *   0                   1                   2                   3
         *   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |    CNAME=1    |     length    | user and domain name        ...
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */
        super::SDES_TYPE_LEN + super::SDES_OCTET_COUNT_LEN + self.text.len()
    }

    /// Marshal encodes the SourceDescriptionItem in binary
    pub fn marshal(&self) -> Result<BytesMut, Error> {
        /*
         *   0                   1                   2                   3
         *   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |    CNAME=1    |     length    | user and domain name        ...
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        if self.sdes_type == super::SDESType::SDESEnd {
            return Err(Error::SDESMissingType);
        }

        let mut raw_packet = BytesMut::new();

        raw_packet.resize(super::SDES_TYPE_LEN + super::SDES_OCTET_COUNT_LEN, 0u8);

        raw_packet[super::SDES_TYPE_OFFSET] = self.sdes_type as u8;

        let text_bytes = self.text.as_bytes();
        let octet_count = text_bytes.len();

        if octet_count > super::SDES_MAX_OCTET_COUNT {
            return Err(Error::SDESTextTooLong);
        }

        raw_packet[super::SDES_OCTET_COUNT_OFFSET] = octet_count as u8;

        raw_packet.extend_from_slice(text_bytes);

        Ok(raw_packet)
    }

    /// Unmarshal decodes the SourceDescriptionItem from binary
    pub fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        /*
         *   0                   1                   2                   3
         *   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         *  |    CNAME=1    |     length    | user and domain name        ...
         *  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        if raw_packet.len() < (super::SDES_TYPE_LEN + super::SDES_OCTET_COUNT_LEN) {
            return Err(Error::PacketTooShort);
        }

        self.sdes_type = super::SDESType::from(raw_packet[super::SDES_TYPE_OFFSET]);

        let octet_count = raw_packet[super::SDES_OCTET_COUNT_OFFSET] as usize;
        if super::SDES_TEXT_OFFSET + octet_count as usize > raw_packet.len() {
            return Err(Error::PacketTooShort);
        }

        let text_bytes =
            &raw_packet[super::SDES_TEXT_OFFSET..super::SDES_TEXT_OFFSET + octet_count];

        self.text = match String::from_utf8(text_bytes.to_vec()) {
            Ok(e) => e,

            Err(e) => {
                return Err(Error::Other(format!(
                    "Error converting byte to string, error:{:?}",
                    e,
                )))
            }
        };

        Ok(())
    }
}

// A SourceDescription (SDES) packet describes the sources in an RTP stream.
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
    /// Unmarshal decodes the SourceDescription from binary
    fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
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

        let mut h = header::Header::default();

        h.unmarshal(raw_packet)?;

        if h.packet_type != header::PacketType::SourceDescription {
            return Err(Error::WrongType);
        }

        let mut i = header::HEADER_LENGTH;

        while i < raw_packet.len() {
            let mut chunk = source_description::SourceDescriptionChunk::default();

            chunk.unmarshal(&mut raw_packet[i..].into())?;

            i += chunk.len();
            self.chunks.push(chunk);
        }

        if self.chunks.len() != h.count as usize {
            return Err(Error::InvalidHeader);
        }

        Ok(())
    }

    /// Marshal encodes the SourceDescription in binary
    fn marshal(&self) -> Result<BytesMut, Error> {
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

        let mut raw_packet = BytesMut::new();
        raw_packet.resize(self.len(), 0u8);

        let packet_body = &mut raw_packet[header::HEADER_LENGTH..];

        let mut chunk_offset = 0;

        for c in &self.chunks {
            let data = c.marshal()?;

            packet_body[chunk_offset..chunk_offset + data.len()].copy_from_slice(&data);
            chunk_offset += data.len();
        }

        if self.chunks.len() > header::COUNT_MAX {
            return Err(Error::TooManyChunks);
        }

        let header_data = self.header().marshal()?;

        raw_packet[..header_data.len()].copy_from_slice(&header_data);

        Ok(raw_packet)
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        self.chunks.iter().map(|x| x.source).collect()
    }

    fn trait_eq(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<SourceDescription>()
            .map_or(false, |a| self == a)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SourceDescription {
    fn len(&self) -> usize {
        let mut chunks_length = 0;
        for c in &self.chunks {
            chunks_length += c.len();
        }

        header::HEADER_LENGTH + chunks_length
    }

    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> header::Header {
        let l = self.len() + get_padding(self.len());
        header::Header {
            padding: get_padding(self.len()) != 0,
            count: self.chunks.len() as u8,
            packet_type: header::PacketType::SourceDescription,
            length: ((l / 4) - 1) as u16,
        }
    }
}
