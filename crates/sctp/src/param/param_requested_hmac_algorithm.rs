use super::{param_header::*, param_type::*, *};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub(crate) enum HmacAlgorithm {
    HmacResv1 = 0,
    HmacSha128 = 1,
    HmacResv2 = 2,
    HmacSha256 = 3,
    Unknown,
}

impl fmt::Display for HmacAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let others = format!("Unknown HMAC Algorithm type: {}", self);
        let s = match *self {
            HmacAlgorithm::HmacResv1 => "HMAC Reserved (0x00)",
            HmacAlgorithm::HmacSha128 => "HMAC SHA-128",
            HmacAlgorithm::HmacResv2 => "HMAC Reserved (0x02)",
            HmacAlgorithm::HmacSha256 => "HMAC SHA-256",
            _ => others.as_str(),
        };
        write!(f, "{}", s)
    }
}

impl From<u16> for HmacAlgorithm {
    fn from(v: u16) -> HmacAlgorithm {
        match v {
            0 => HmacAlgorithm::HmacResv1,
            1 => HmacAlgorithm::HmacSha128,
            2 => HmacAlgorithm::HmacResv2,
            3 => HmacAlgorithm::HmacSha256,
            _ => HmacAlgorithm::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParamRequestedHmacAlgorithm {
    pub(crate) available_algorithms: Vec<HmacAlgorithm>,
}

impl Param for ParamRequestedHmacAlgorithm {
    fn unmarshal(raw: &Bytes) -> Result<Self, Error> {
        let _ = ParamHeader::unmarshal(raw)?;

        let reader = &mut raw.slice(PARAM_HEADER_LENGTH..);

        let mut available_algorithms = vec![];
        let mut offset = PARAM_HEADER_LENGTH;
        while offset + 1 < raw.len() {
            let a: HmacAlgorithm = reader.get_u16().into();
            if a == HmacAlgorithm::HmacSha128 || a == HmacAlgorithm::HmacSha256 {
                available_algorithms.push(a);
            } else {
                return Err(Error::ErrInvalidAlgorithmType);
            }

            offset += 2;
        }

        Ok(ParamRequestedHmacAlgorithm {
            available_algorithms,
        })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error> {
        self.header().marshal_to(buf)?;
        for a in &self.available_algorithms {
            buf.put_u16(*a as u16);
        }
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        2 * self.available_algorithms.len()
    }
}

impl ParamRequestedHmacAlgorithm {
    pub(crate) fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::ReqHmacAlgo,
            value_length: self.value_length() as u16,
        }
    }
}
