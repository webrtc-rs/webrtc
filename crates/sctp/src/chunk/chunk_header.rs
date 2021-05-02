use super::{chunk_type::*, *};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

///chunkHeader represents a SCTP Chunk header, defined in https://tools.ietf.org/html/rfc4960#section-3.2
///The figure below illustrates the field format for the chunks to be
///transmitted in the SCTP packet.  Each chunk is formatted with a Chunk
///Type field, a chunk-specific Flag field, a Chunk Length field, and a
///Value field.
///
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|   Chunk Type  | Chunk  Flags  |        Chunk Length           |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|                          Chunk Value                          |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Clone)]
pub(crate) struct ChunkHeader {
    pub(crate) typ: ChunkType,
    pub(crate) flags: u8,
    pub(crate) value_length: u16,
}

pub(crate) const CHUNK_HEADER_SIZE: usize = 4;

/// makes ChunkHeader printable
impl fmt::Display for ChunkHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.typ)
    }
}

impl Chunk for ChunkHeader {
    fn header(&self) -> ChunkHeader {
        self.clone()
    }

    fn unmarshal(buf: &Bytes) -> Result<Self, Error> {
        if buf.len() < CHUNK_HEADER_SIZE {
            return Err(Error::ErrChunkHeaderTooSmall);
        }

        let reader = &mut buf.clone();

        let typ: ChunkType = reader.get_u8().into();
        let flags = reader.get_u8();
        let length = reader.get_u16();

        if length < CHUNK_HEADER_SIZE as u16 {
            return Err(Error::ErrChunkHeaderInvalidLength);
        }

        // Length includes Chunk header
        let length_after_value = buf.len() as isize - length as isize;
        if length_after_value < 0 {
            return Err(Error::ErrChunkHeaderNotEnoughSpace);
        } else if length_after_value < 4 {
            // https://tools.ietf.org/html/rfc4960#section-3.2
            // The Chunk Length field does not count any chunk PADDING.
            // Chunks (including Type, Length, and Value fields) are padded out
            // by the sender with all zero bytes to be a multiple of 4 bytes
            // long.  This PADDING MUST NOT be more than 3 bytes in total.  The
            // Chunk Length value does not include terminating PADDING of the
            // chunk.  However, it does include PADDING of any variable-length
            // parameter except the last parameter in the chunk.  The receiver
            // MUST ignore the PADDING.
            for _ in 0..length_after_value {
                if reader.get_u8() != 0 {
                    return Err(Error::ErrChunkHeaderPaddingNonZero);
                }
            }
        } else {
            return Err(Error::ErrChunkHeaderInvalidLength);
        }

        Ok(ChunkHeader {
            typ,
            flags,
            value_length: length - CHUNK_HEADER_SIZE as u16,
        })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize, Error> {
        writer.put_u8(self.typ as u8);
        writer.put_u8(self.flags);
        writer.put_u16(self.value_length + CHUNK_HEADER_SIZE as u16);
        Ok(writer.len())
    }

    fn check(&self) -> Result<(), Error> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        self.value_length as usize
    }
}
