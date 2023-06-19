use std::any::Any;
use std::fmt::{Debug, Display, Formatter};

use bytes::{Bytes, BytesMut};

use crate::param::param_header::{ParamHeader, PARAM_HEADER_LENGTH};
use crate::param::param_type::ParamType;
use crate::param::Param;

/// This type is meant to represent ANY parameter for un/remarshaling purposes, where we do not have a more specific type for it.
/// This means we do not really understand the semantics of the param but can represent it.
///
/// This is useful for usage in e.g.`ParamUnrecognized` where we want to report some unrecognized params back to the sender.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamUnknown {
    typ: u16,
    value: Bytes,
}

impl Display for ParamUnknown {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParamUnknown( {} {:?} )", self.header(), self.value)
    }
}

impl Param for ParamUnknown {
    fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::Unknown {
                param_type: self.typ,
            },
            value_length: self.value.len() as u16,
        }
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn unmarshal(raw: &Bytes) -> crate::error::Result<Self>
    where
        Self: Sized,
    {
        let header = ParamHeader::unmarshal(raw)?;
        let value = raw.slice(PARAM_HEADER_LENGTH..PARAM_HEADER_LENGTH + header.value_length());
        Ok(Self {
            typ: header.typ.into(),
            value,
        })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> crate::error::Result<usize> {
        self.header().marshal_to(buf)?;
        buf.extend(self.value.clone());
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        self.value.len()
    }

    fn clone_to(&self) -> Box<dyn Param + Send + Sync> {
        Box::new(self.clone())
    }
}
