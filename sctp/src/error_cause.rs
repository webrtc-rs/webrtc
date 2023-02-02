use crate::error::{Error, Result};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;

/// errorCauseCode is a cause code that appears in either a ERROR or ABORT chunk
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub(crate) struct ErrorCauseCode(pub(crate) u16);

pub(crate) const INVALID_STREAM_IDENTIFIER: ErrorCauseCode = ErrorCauseCode(1);
pub(crate) const MISSING_MANDATORY_PARAMETER: ErrorCauseCode = ErrorCauseCode(2);
pub(crate) const STALE_COOKIE_ERROR: ErrorCauseCode = ErrorCauseCode(3);
pub(crate) const OUT_OF_RESOURCE: ErrorCauseCode = ErrorCauseCode(4);
pub(crate) const UNRESOLVABLE_ADDRESS: ErrorCauseCode = ErrorCauseCode(5);
pub(crate) const UNRECOGNIZED_CHUNK_TYPE: ErrorCauseCode = ErrorCauseCode(6);
pub(crate) const INVALID_MANDATORY_PARAMETER: ErrorCauseCode = ErrorCauseCode(7);
pub(crate) const UNRECOGNIZED_PARAMETERS: ErrorCauseCode = ErrorCauseCode(8);
pub(crate) const NO_USER_DATA: ErrorCauseCode = ErrorCauseCode(9);
pub(crate) const COOKIE_RECEIVED_WHILE_SHUTTING_DOWN: ErrorCauseCode = ErrorCauseCode(10);
pub(crate) const RESTART_OF_AN_ASSOCIATION_WITH_NEW_ADDRESSES: ErrorCauseCode = ErrorCauseCode(11);
pub(crate) const USER_INITIATED_ABORT: ErrorCauseCode = ErrorCauseCode(12);
pub(crate) const PROTOCOL_VIOLATION: ErrorCauseCode = ErrorCauseCode(13);

impl fmt::Display for ErrorCauseCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let others = format!("Unknown CauseCode: {}", self.0);
        let s = match *self {
            INVALID_STREAM_IDENTIFIER => "Invalid Stream Identifier",
            MISSING_MANDATORY_PARAMETER => "Missing Mandatory Parameter",
            STALE_COOKIE_ERROR => "Stale Cookie Error",
            OUT_OF_RESOURCE => "Out Of Resource",
            UNRESOLVABLE_ADDRESS => "Unresolvable IP",
            UNRECOGNIZED_CHUNK_TYPE => "Unrecognized Chunk Type",
            INVALID_MANDATORY_PARAMETER => "Invalid Mandatory Parameter",
            UNRECOGNIZED_PARAMETERS => "Unrecognized Parameters",
            NO_USER_DATA => "No User Data",
            COOKIE_RECEIVED_WHILE_SHUTTING_DOWN => "Cookie Received While Shutting Down",
            RESTART_OF_AN_ASSOCIATION_WITH_NEW_ADDRESSES => {
                "Restart Of An Association With New Addresses"
            }
            USER_INITIATED_ABORT => "User Initiated Abort",
            PROTOCOL_VIOLATION => "Protocol Violation",
            _ => others.as_str(),
        };
        write!(f, "{s}")
    }
}

/// ErrorCauseHeader represents the shared header that is shared by all error causes
#[derive(Debug, Clone, Default)]
pub(crate) struct ErrorCause {
    pub(crate) code: ErrorCauseCode,
    pub(crate) raw: Bytes,
}

/// ErrorCauseInvalidMandatoryParameter represents an SCTP error cause
pub(crate) type ErrorCauseInvalidMandatoryParameter = ErrorCause;

/// ErrorCauseUnrecognizedChunkType represents an SCTP error cause
pub(crate) type ErrorCauseUnrecognizedChunkType = ErrorCause;

///
/// This error cause MAY be included in ABORT chunks that are sent
/// because an SCTP endpoint detects a protocol violation of the peer
/// that is not covered by the error causes described in Section 3.3.10.1
/// to Section 3.3.10.12.  An implementation MAY provide additional
/// information specifying what kind of protocol violation has been
/// detected.
///      0                   1                   2                   3
///      0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///     |         Cause Code=13         |      Cause Length=Variable    |
///     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///     /                    Additional Information                     /
///     \                                                               \
///     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
pub(crate) type ErrorCauseProtocolViolation = ErrorCause;

pub(crate) const ERROR_CAUSE_HEADER_LENGTH: usize = 4;

/// makes ErrorCauseHeader printable
impl fmt::Display for ErrorCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code)
    }
}

impl ErrorCause {
    pub(crate) fn unmarshal(buf: &Bytes) -> Result<Self> {
        if buf.len() < ERROR_CAUSE_HEADER_LENGTH {
            return Err(Error::ErrErrorCauseTooSmall);
        }

        let reader = &mut buf.clone();

        let code = ErrorCauseCode(reader.get_u16());
        let len = reader.get_u16();

        if len < ERROR_CAUSE_HEADER_LENGTH as u16 {
            return Err(Error::ErrErrorCauseTooSmall);
        }
        if buf.len() < len as usize {
            return Err(Error::ErrErrorCauseTooSmall);
        }

        let value_length = len as usize - ERROR_CAUSE_HEADER_LENGTH;

        let raw = buf.slice(ERROR_CAUSE_HEADER_LENGTH..ERROR_CAUSE_HEADER_LENGTH + value_length);

        Ok(ErrorCause { code, raw })
    }

    pub(crate) fn marshal(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.length());
        let _ = self.marshal_to(&mut buf);
        buf.freeze()
    }

    pub(crate) fn marshal_to(&self, writer: &mut BytesMut) -> usize {
        let len = self.raw.len() + ERROR_CAUSE_HEADER_LENGTH;
        writer.put_u16(self.code.0);
        writer.put_u16(len as u16);
        writer.extend(self.raw.clone());
        writer.len()
    }

    pub(crate) fn length(&self) -> usize {
        self.raw.len() + ERROR_CAUSE_HEADER_LENGTH
    }

    pub(crate) fn error_cause_code(&self) -> ErrorCauseCode {
        self.code
    }
}
