use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::param_header::*;
use super::param_type::*;
use super::*;
use crate::chunk::chunk_type::*;

#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct ParamChunkList {
    pub(crate) chunk_types: Vec<ChunkType>,
}

impl fmt::Display for ParamChunkList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}",
            self.header(),
            self.chunk_types
                .iter()
                .map(|ct| ct.to_string())
                .collect::<Vec<String>>()
                .join(" ")
        )
    }
}

impl Param for ParamChunkList {
    fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::ChunkList,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ParamHeader::unmarshal(raw)?;

        if header.typ != ParamType::ChunkList {
            return Err(Error::ErrParamTypeUnexpected);
        }

        let reader =
            &mut raw.slice(PARAM_HEADER_LENGTH..PARAM_HEADER_LENGTH + header.value_length());

        let mut chunk_types = vec![];
        while reader.has_remaining() {
            chunk_types.push(ChunkType(reader.get_u8()));
        }

        Ok(ParamChunkList { chunk_types })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(buf)?;
        for ct in &self.chunk_types {
            buf.put_u8(ct.0);
        }
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        self.chunk_types.len()
    }

    fn clone_to(&self) -> Box<dyn Param + Send + Sync> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
