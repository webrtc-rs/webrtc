use super::{chunk_header::*, chunk_type::*, *};
use crate::param::{param_header::*, *};

use crate::util::get_padding_size;
use bytes::{Bytes, BytesMut};
use std::fmt;

///https://tools.ietf.org/html/rfc6525#section-3.1
///chunkReconfig represents an SCTP Chunk used to reconfigure streams.
///
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///| Type = 130    |  Chunk Flags  |      Chunk Length             |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|                  Re-configuration Parameter                   |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                                                               |
///|             Re-configuration Parameter (optional)             |
///|                                                               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
pub(crate) struct ChunkReconfig {
    param_a: Box<dyn Param>,
    param_b: Option<Box<dyn Param>>,
}

/// makes chunkReconfig printable
impl fmt::Display for ChunkReconfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = format!("Param A:\n {}", self.param_a);
        if let Some(param_b) = &self.param_b {
            res += format!("Param B:\n {}", param_b).as_str()
        }
        write!(f, "{}", res)
    }
}

impl Chunk for ChunkReconfig {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: ChunkType::Reconfig,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != ChunkType::Reconfig {
            return Err(Error::ErrChunkTypeNotReconfig);
        }

        let param_a = build_param(&raw.slice(CHUNK_HEADER_SIZE..))?;

        let padding = get_padding_size(PARAM_HEADER_LENGTH + param_a.value_length());
        let offset = CHUNK_HEADER_SIZE + PARAM_HEADER_LENGTH + param_a.value_length() + padding;
        let param_b = if raw.len() > offset {
            Some(build_param(&raw.slice(offset..))?)
        } else {
            None
        };

        Ok(ChunkReconfig { param_a, param_b })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize, Error> {
        self.header().marshal_to(writer)?;

        writer.extend(self.param_a.marshal()?);
        if let Some(param_b) = &self.param_b {
            // Pad param A
            let padding = get_padding_size(PARAM_HEADER_LENGTH + self.param_a.value_length());
            writer.extend(vec![0u8; padding]);
            writer.extend(param_b.marshal()?);
        }
        Ok(writer.len())
    }

    fn check(&self) -> Result<(), Error> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        let mut l = self.param_a.value_length() + PARAM_HEADER_LENGTH;
        if let Some(param_b) = &self.param_b {
            l += param_b.value_length() + PARAM_HEADER_LENGTH;
        }
        l
    }
}
