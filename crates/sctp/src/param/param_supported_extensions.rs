use super::{param_header::*, param_type::*, *};
use crate::chunk::chunk_type::*;

use bytes::{Buf, BufMut, Bytes, BytesMut};

#[derive(Debug, Clone)]
pub(crate) struct ParamSupportedExtensions {
    pub(crate) chunk_types: Vec<ChunkType>,
}

impl Param for ParamSupportedExtensions {
    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let _ = ParamHeader::unmarshal(raw)?;

        let reader = &mut raw.slice(PARAM_HEADER_LENGTH..);

        let mut chunk_types = vec![];
        while reader.has_remaining() {
            chunk_types.push(reader.get_u8().into())
        }

        Ok(ParamSupportedExtensions { chunk_types })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error> {
        self.header().marshal_to(buf)?;
        for ct in &self.chunk_types {
            buf.put_u8(*ct as u8);
        }
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        self.chunk_types.len()
    }
}

impl ParamSupportedExtensions {
    pub(crate) fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::SupportedExt,
            value_length: self.value_length() as u16,
        }
    }
}
