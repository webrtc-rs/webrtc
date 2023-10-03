use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::content::*;
use crate::error::*;

pub const RECORD_LAYER_HEADER_SIZE: usize = 13;
pub const MAX_SEQUENCE_NUMBER: u64 = 0x0000FFFFFFFFFFFF;

pub const DTLS1_2MAJOR: u8 = 0xfe;
pub const DTLS1_2MINOR: u8 = 0xfd;

pub const DTLS1_0MAJOR: u8 = 0xfe;
pub const DTLS1_0MINOR: u8 = 0xff;

// VERSION_DTLS12 is the DTLS version in the same style as
// VersionTLSXX from crypto/tls
pub const VERSION_DTLS12: u16 = 0xfefd;

pub const PROTOCOL_VERSION1_0: ProtocolVersion = ProtocolVersion {
    major: DTLS1_0MAJOR,
    minor: DTLS1_0MINOR,
};
pub const PROTOCOL_VERSION1_2: ProtocolVersion = ProtocolVersion {
    major: DTLS1_2MAJOR,
    minor: DTLS1_2MINOR,
};

// https://tools.ietf.org/html/rfc4346#section-6.2.1
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct RecordLayerHeader {
    pub content_type: ContentType,
    pub protocol_version: ProtocolVersion,
    pub epoch: u16,
    pub sequence_number: u64, // uint48 in spec
    pub content_len: u16,
}

impl RecordLayerHeader {
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        if self.sequence_number > MAX_SEQUENCE_NUMBER {
            return Err(Error::ErrSequenceNumberOverflow);
        }

        writer.write_u8(self.content_type as u8)?;
        writer.write_u8(self.protocol_version.major)?;
        writer.write_u8(self.protocol_version.minor)?;
        writer.write_u16::<BigEndian>(self.epoch)?;

        let be: [u8; 8] = self.sequence_number.to_be_bytes();
        writer.write_all(&be[2..])?; // uint48 in spec

        writer.write_u16::<BigEndian>(self.content_len)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let content_type = reader.read_u8()?.into();
        let major = reader.read_u8()?;
        let minor = reader.read_u8()?;
        let epoch = reader.read_u16::<BigEndian>()?;

        // SequenceNumber is stored as uint48, make into uint64
        let mut be: [u8; 8] = [0u8; 8];
        reader.read_exact(&mut be[2..])?;
        let sequence_number = u64::from_be_bytes(be);

        let protocol_version = ProtocolVersion { major, minor };
        if protocol_version != PROTOCOL_VERSION1_0 && protocol_version != PROTOCOL_VERSION1_2 {
            return Err(Error::ErrUnsupportedProtocolVersion);
        }
        let content_len = reader.read_u16::<BigEndian>()?;

        Ok(RecordLayerHeader {
            content_type,
            protocol_version,
            epoch,
            sequence_number,
            content_len,
        })
    }
}
