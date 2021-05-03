use super::{chunk_header::*, chunk_type::*, *};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

///chunkShutdown represents an SCTP Chunk of type chunkShutdown
///
///0                   1                   2                   3
///0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Type = 7    | Chunk  Flags  |      Length = 8               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                      Cumulative TSN Ack                       |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Clone)]
pub(crate) struct ChunkShutdown {
    cumulative_tsn_ack: u32,
}

pub(crate) const CUMULATIVE_TSN_ACK_LENGTH: usize = 4;

/// makes chunkShutdown printable
impl fmt::Display for ChunkShutdown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.header())
    }
}

impl Chunk for ChunkShutdown {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: ChunkType::Shutdown,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != ChunkType::Shutdown {
            return Err(Error::ErrChunkTypeNotShutdown);
        }

        if raw.len() != CHUNK_HEADER_SIZE + CUMULATIVE_TSN_ACK_LENGTH {
            return Err(Error::ErrInvalidChunkSize);
        }

        let reader = &mut raw.slice(CHUNK_HEADER_SIZE..);

        let cumulative_tsn_ack = reader.get_u32();

        Ok(ChunkShutdown { cumulative_tsn_ack })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize, Error> {
        self.header().marshal_to(writer)?;
        writer.put_u32(self.cumulative_tsn_ack);
        Ok(writer.len())
    }

    fn check(&self) -> Result<(), Error> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        CUMULATIVE_TSN_ACK_LENGTH
    }
}
