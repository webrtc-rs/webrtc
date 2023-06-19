use std::fmt;

use bytes::{Bytes, BytesMut};

use super::chunk_header::*;
use super::chunk_type::*;
use super::*;

///chunkShutdownAck represents an SCTP Chunk of type chunkShutdownAck
///
///0                   1                   2                   3
///0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 8    | Chunk  Flags  |      Length = 4               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Default, Debug, Clone)]
pub(crate) struct ChunkShutdownAck;

/// makes chunkShutdownAck printable
impl fmt::Display for ChunkShutdownAck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.header())
    }
}

impl Chunk for ChunkShutdownAck {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: CT_SHUTDOWN_ACK,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != CT_SHUTDOWN_ACK {
            return Err(Error::ErrChunkTypeNotShutdownAck);
        }

        Ok(ChunkShutdownAck {})
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(writer)?;
        Ok(writer.len())
    }

    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        0
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
