use super::{chunk_header::*, chunk_type::*, *};

use bytes::{Bytes, BytesMut};
use std::fmt;

///chunkShutdownComplete represents an SCTP Chunk of type chunkShutdownComplete
///
///0                   1                   2                   3
///0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 14   |Reserved     |T|      Length = 4               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Clone)]
pub(crate) struct ChunkShutdownComplete;

/// makes chunkShutdownComplete printable
impl fmt::Display for ChunkShutdownComplete {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.header())
    }
}

impl Chunk for ChunkShutdownComplete {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: ChunkType::ShutdownComplete,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != ChunkType::ShutdownComplete {
            return Err(Error::ErrChunkTypeNotShutdownComplete);
        }

        Ok(ChunkShutdownComplete {})
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize, Error> {
        self.header().marshal_to(writer)?;
        Ok(writer.len())
    }

    fn check(&self) -> Result<(), Error> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        0
    }
}
