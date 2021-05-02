use super::{param_header::*, param_type::*, *};

use bytes::{Bytes, BytesMut};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParamRandom {
    pub(crate) random_data: Bytes,
}

impl Param for ParamRandom {
    fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::Random,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let _ = ParamHeader::unmarshal(raw)?;
        let random_data = raw.slice(PARAM_HEADER_LENGTH..);
        Ok(ParamRandom { random_data })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error> {
        self.header().marshal_to(buf)?;
        buf.extend(self.random_data.clone());
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        self.random_data.len()
    }
}
