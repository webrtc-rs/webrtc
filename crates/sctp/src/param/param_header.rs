use super::{param_type::*, *};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

pub(crate) struct ParamHeader {
    typ: ParamType,
    len: usize,
}

pub(crate) const PARAM_HEADER_LENGTH: usize = 4;

/// String makes paramHeader printable
impl fmt::Display for ParamHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.typ, self.len)
    }
}

impl Param for ParamHeader {
    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        if raw.len() < PARAM_HEADER_LENGTH {
            return Err(Error::ErrParamHeaderTooShort);
        }

        let reader = &mut raw.clone();

        let typ = ParamType(reader.get_u16());

        let len = reader.get_u16() as usize;
        if len < PARAM_HEADER_LENGTH || raw.len() < len {
            return Err(Error::ErrParamHeaderTooShort);
        }

        Ok(ParamHeader { typ, len })
    }

    fn marshal_to(&self, writer: &mut BytesMut) -> Result<usize, Error> {
        writer.put_u16(self.typ.0);
        writer.put_u16(self.len as u16);
        Ok(writer.len())
    }

    fn length(&self) -> usize {
        self.len
    }
}
