use std::fmt;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::param_type::*;
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParamHeader {
    pub(crate) typ: ParamType,
    pub(crate) value_length: u16,
}

pub(crate) const PARAM_HEADER_LENGTH: usize = 4;

/// String makes paramHeader printable
impl fmt::Display for ParamHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.typ)
    }
}

impl Param for ParamHeader {
    fn header(&self) -> ParamHeader {
        self.clone()
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        if raw.len() < PARAM_HEADER_LENGTH {
            return Err(Error::ErrParamHeaderTooShort);
        }

        let reader = &mut raw.clone();

        let typ: ParamType = reader.get_u16().into();

        let len = reader.get_u16() as usize;
        if len < PARAM_HEADER_LENGTH || raw.len() < len {
            return Err(Error::ErrParamHeaderTooShort);
        }

        Ok(ParamHeader {
            typ,
            value_length: (len - PARAM_HEADER_LENGTH) as u16,
        })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize> {
        writer.put_u16(self.typ.into());
        writer.put_u16(self.value_length + PARAM_HEADER_LENGTH as u16);
        Ok(writer.len())
    }

    fn value_length(&self) -> usize {
        self.value_length as usize
    }

    fn clone_to(&self) -> Box<dyn Param + Send + Sync> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
