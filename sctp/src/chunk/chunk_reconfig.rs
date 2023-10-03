use std::fmt;

use bytes::{Bytes, BytesMut};

use super::chunk_header::*;
use super::chunk_type::*;
use super::*;
use crate::param::param_header::*;
use crate::param::*;
use crate::util::get_padding_size;

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
#[derive(Default, Debug)]
pub(crate) struct ChunkReconfig {
    pub(crate) param_a: Option<Box<dyn Param + Send + Sync>>,
    pub(crate) param_b: Option<Box<dyn Param + Send + Sync>>,
}

impl Clone for ChunkReconfig {
    fn clone(&self) -> Self {
        ChunkReconfig {
            param_a: self.param_a.as_ref().cloned(),
            param_b: self.param_b.as_ref().cloned(),
        }
    }
}

/// makes chunkReconfig printable
impl fmt::Display for ChunkReconfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = String::new();
        if let Some(param_a) = &self.param_a {
            res += format!("Param A:\n {param_a}").as_str();
        }
        if let Some(param_b) = &self.param_b {
            res += format!("Param B:\n {param_b}").as_str()
        }
        write!(f, "{res}")
    }
}

impl Chunk for ChunkReconfig {
    fn header(&self) -> ChunkHeader {
        ChunkHeader {
            typ: CT_RECONFIG,
            flags: 0,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ChunkHeader::unmarshal(raw)?;

        if header.typ != CT_RECONFIG {
            return Err(Error::ErrChunkTypeNotReconfig);
        }

        let param_a =
            build_param(&raw.slice(CHUNK_HEADER_SIZE..CHUNK_HEADER_SIZE + header.value_length()))?;

        let padding = get_padding_size(PARAM_HEADER_LENGTH + param_a.value_length());
        let offset = CHUNK_HEADER_SIZE + PARAM_HEADER_LENGTH + param_a.value_length() + padding;
        let param_b = if CHUNK_HEADER_SIZE + header.value_length() > offset {
            Some(build_param(
                &raw.slice(offset..CHUNK_HEADER_SIZE + header.value_length()),
            )?)
        } else {
            None
        };

        Ok(ChunkReconfig {
            param_a: Some(param_a),
            param_b,
        })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(writer)?;

        let param_a_value_length = if let Some(param_a) = &self.param_a {
            writer.extend(param_a.marshal()?);
            param_a.value_length()
        } else {
            return Err(Error::ErrChunkReconfigInvalidParamA);
        };

        if let Some(param_b) = &self.param_b {
            // Pad param A
            let padding = get_padding_size(PARAM_HEADER_LENGTH + param_a_value_length);
            writer.extend(vec![0u8; padding]);
            writer.extend(param_b.marshal()?);
        }
        Ok(writer.len())
    }

    fn check(&self) -> Result<()> {
        Ok(())
    }

    fn value_length(&self) -> usize {
        let mut l = PARAM_HEADER_LENGTH;
        let param_a_value_length = if let Some(param_a) = &self.param_a {
            l += param_a.value_length();
            param_a.value_length()
        } else {
            0
        };
        if let Some(param_b) = &self.param_b {
            let padding = get_padding_size(PARAM_HEADER_LENGTH + param_a_value_length);
            l += PARAM_HEADER_LENGTH + param_b.value_length() + padding;
        }
        l
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
