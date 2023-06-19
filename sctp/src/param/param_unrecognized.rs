use std::any::Any;
use std::fmt::{Debug, Display, Formatter};

use bytes::{Bytes, BytesMut};

use crate::param::param_header::PARAM_HEADER_LENGTH;
use crate::param::param_type::ParamType;
use crate::param::{build_param, Param, ParamHeader};

/// This is the parameter type used to report unrecognized parameters in e.g. init chunks back to the sender in the init ack.
/// The contained param is likely to be a `ParamUnknown` but might be something more specific.
#[derive(Clone, Debug)]
pub struct ParamUnrecognized {
    param: Box<dyn Param + Send + Sync>,
}

impl ParamUnrecognized {
    pub(crate) fn wrap(param: Box<dyn Param + Send + Sync>) -> Self {
        Self { param }
    }
}

impl Display for ParamUnrecognized {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("UnrecognizedParam")?;
        Display::fmt(&self.param, f)
    }
}

impl Param for ParamUnrecognized {
    fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::UnrecognizedParam,
            value_length: self.value_length() as u16,
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
        let raw_param = raw.slice(PARAM_HEADER_LENGTH..PARAM_HEADER_LENGTH + header.value_length());
        let param = build_param(&raw_param)?;
        Ok(Self { param })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> crate::error::Result<usize> {
        self.header().marshal_to(buf)?;
        self.param.marshal_to(buf)?;
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        self.param.value_length() + PARAM_HEADER_LENGTH
    }

    fn clone_to(&self) -> Box<dyn Param + Send + Sync> {
        Box::new(self.clone())
    }
}
