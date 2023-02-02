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
        let s = match *self {
            HmacAlgorithm::HmacResv1 => "HMAC Reserved (0x00)",
            HmacAlgorithm::HmacSha128 => "HMAC SHA-128",
            HmacAlgorithm::HmacResv2 => "HMAC Reserved (0x02)",
            HmacAlgorithm::HmacSha256 => "HMAC SHA-256",
            _ => "Unknown HMAC Algorithm",
        };
        write!(f, "{s}")
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

#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct ParamRequestedHmacAlgorithm {
    pub(crate) available_algorithms: Vec<HmacAlgorithm>,
}

impl fmt::Display for ParamRequestedHmacAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}",
            self.header(),
            self.available_algorithms
                .iter()
                .map(|ct| ct.to_string())
                .collect::<Vec<String>>()
                .join(" "),
        )
    }
}

impl Param for ParamRequestedHmacAlgorithm {
    fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::ReqHmacAlgo,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ParamHeader::unmarshal(raw)?;

        let reader =
            &mut raw.slice(PARAM_HEADER_LENGTH..PARAM_HEADER_LENGTH + header.value_length());

        let mut available_algorithms = vec![];
        let mut offset = 0;
        while offset + 1 < header.value_length() {
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

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(buf)?;
        for a in &self.available_algorithms {
            buf.put_u16(*a as u16);
        }
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        2 * self.available_algorithms.len()
    }

    fn clone_to(&self) -> Box<dyn Param + Send + Sync> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
