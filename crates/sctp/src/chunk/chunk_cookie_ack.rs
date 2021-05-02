use super::{chunk_header::*, chunk_type::*, *};

use bytes::{Bytes, BytesMut};
use std::fmt;

/// chunkCookieAck represents an SCTP Chunk of type chunkCookieAck
///
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |   Type = 11   |Chunk  Flags   |     Length = 4                |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Clone)]
pub(crate) struct ChunkCookieAck;

/// makes ChunkCookieAck printable
impl fmt::Display for ChunkCookieAck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.header())
    }
}

impl Chunk for ChunkCookieAck {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: ChunkType::CookieAck,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != ChunkType::CookieAck {
            return Err(Error::ErrChunkTypeNotCookieAck);
        }

        Ok(ChunkCookieAck {})
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error> {
        self.header().marshal_to(buf)?;
        Ok(buf.len())
    }

    fn check(&self) -> Result<(), Error> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        0
    }
}
