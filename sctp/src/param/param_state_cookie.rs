use std::fmt;

use bytes::{Bytes, BytesMut};
use rand::Rng;

use super::param_header::*;
use super::param_type::*;
use super::*;

#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct ParamStateCookie {
    pub(crate) cookie: Bytes,
}

/// String makes paramStateCookie printable
impl fmt::Display for ParamStateCookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.header(), self.cookie)
    }
}

impl Param for ParamStateCookie {
    fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::StateCookie,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ParamHeader::unmarshal(raw)?;
        let cookie = raw.slice(PARAM_HEADER_LENGTH..PARAM_HEADER_LENGTH + header.value_length());
        Ok(ParamStateCookie { cookie })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(buf)?;
        buf.extend(self.cookie.clone());
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        self.cookie.len()
    }

    fn clone_to(&self) -> Box<dyn Param + Send + Sync> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

impl ParamStateCookie {
    pub(crate) fn new() -> Self {
        let mut cookie = BytesMut::new();
        cookie.resize(32, 0);
        rand::thread_rng().fill(cookie.as_mut());

        ParamStateCookie {
            cookie: cookie.freeze(),
        }
    }
}
