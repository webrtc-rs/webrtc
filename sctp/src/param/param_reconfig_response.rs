use std::fmt;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::param_header::*;
use super::param_type::*;
use super::*;

#[derive(Default, Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub(crate) enum ReconfigResult {
    SuccessNop = 0,
    SuccessPerformed = 1,
    Denied = 2,
    ErrorWrongSsn = 3,
    ErrorRequestAlreadyInProgress = 4,
    ErrorBadSequenceNumber = 5,
    InProgress = 6,
    #[default]
    Unknown,
}

impl fmt::Display for ReconfigResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ReconfigResult::SuccessNop => "0: Success - Nothing to do",
            ReconfigResult::SuccessPerformed => "1: Success - Performed",
            ReconfigResult::Denied => "2: Denied",
            ReconfigResult::ErrorWrongSsn => "3: Error - Wrong SSN",
            ReconfigResult::ErrorRequestAlreadyInProgress => {
                "4: Error - Request already in progress"
            }
            ReconfigResult::ErrorBadSequenceNumber => "5: Error - Bad Sequence Number",
            ReconfigResult::InProgress => "6: In progress",
            _ => "Unknown ReconfigResult",
        };
        write!(f, "{s}")
    }
}

impl From<u32> for ReconfigResult {
    fn from(v: u32) -> ReconfigResult {
        match v {
            0 => ReconfigResult::SuccessNop,
            1 => ReconfigResult::SuccessPerformed,
            2 => ReconfigResult::Denied,
            3 => ReconfigResult::ErrorWrongSsn,
            4 => ReconfigResult::ErrorRequestAlreadyInProgress,
            5 => ReconfigResult::ErrorBadSequenceNumber,
            6 => ReconfigResult::InProgress,
            _ => ReconfigResult::Unknown,
        }
    }
}

///This parameter is used by the receiver of a Re-configuration Request
///Parameter to respond to the request.
///
///0                   1                   2                   3
///0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|     Parameter Type = 16       |      Parameter Length         |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|         Re-configuration Response Sequence Number             |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                            Result                             |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                   Sender's Next TSN (optional)                |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                  Receiver's Next TSN (optional)               |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct ParamReconfigResponse {
    /// This value is copied from the request parameter and is used by the
    /// receiver of the Re-configuration Response Parameter to tie the
    /// response to the request.
    pub(crate) reconfig_response_sequence_number: u32,
    /// This value describes the result of the processing of the request.
    pub(crate) result: ReconfigResult,
}

impl fmt::Display for ParamReconfigResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}",
            self.header(),
            self.reconfig_response_sequence_number,
            self.result
        )
    }
}

impl Param for ParamReconfigResponse {
    fn header(&self) -> ParamHeader {
        ParamHeader {
            typ: ParamType::ReconfigResp,
            value_length: self.value_length() as u16,
        }
    }

    fn unmarshal(raw: &Bytes) -> Result<Self> {
        let header = ParamHeader::unmarshal(raw)?;

        // validity of value_length is checked in ParamHeader::unmarshal
        if header.value_length < 8 {
            return Err(Error::ErrReconfigRespParamTooShort);
        }

        let reader =
            &mut raw.slice(PARAM_HEADER_LENGTH..PARAM_HEADER_LENGTH + header.value_length());

        let reconfig_response_sequence_number = reader.get_u32();
        let result = reader.get_u32().into();

        Ok(ParamReconfigResponse {
            reconfig_response_sequence_number,
            result,
        })
    }

    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        self.header().marshal_to(buf)?;
        buf.put_u32(self.reconfig_response_sequence_number);
        buf.put_u32(self.result as u32);
        Ok(buf.len())
    }

    fn value_length(&self) -> usize {
        8
    }

    fn clone_to(&self) -> Box<dyn Param + Send + Sync> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
