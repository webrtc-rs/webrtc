use bytes::{Bytes, BytesMut};

use super::param_header::*;
use super::param_type::*;
use super::*;

/// At the initialization of the association, the sender of the INIT or
/// INIT ACK chunk MAY include this OPTIONAL parameter to inform its peer
/// that it is able to support the Forward TSN chunk
///
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|    Parameter Type = 49152     |  Parameter Length = 4         |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct ParamForwardTsnSupported;

impl fmt::Display for ParamForwardTsnSupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.header())
    }
}

impl Param for ParamForwardTsnSupported {
    fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::ForwardTsnSupp,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let _ = ParamHeader::unmarshal(raw)?;
        Ok(ParamForwardTsnSupported {})
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(buf)?;
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        0
    }

    fn clone_to(&self) -> Box<dyn Param + Send + Sync> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
