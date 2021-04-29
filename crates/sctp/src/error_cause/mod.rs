use crate::error::Error;

use bytes::{Bytes, BytesMut};
use std::fmt;

pub(crate) trait ErrorCause: fmt::Display {
    fn unmarshal(raw: &Bytes) -> Result<Self, Error>
    where
        Self: Sized;
    fn marshal(&self) -> Result<Bytes, Error>;
    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error>;
    fn length(&self) -> usize;

    fn error_cause_code(&self) -> ErrorCauseCode;
}

/// buildErrorCause delegates the building of a error cause from raw bytes to the correct structure
/*TODO: func buildErrorCause(raw []byte) (errorCause, error) {
    var e errorCause

    c := errorCauseCode(binary.BigEndian.Uint16(raw[0:]))
    switch c {
    case INVALID_MANDATORY_PARAMETER:
        e = &errorCauseInvalidMandatoryParameter{}
    case UNRECOGNIZED_CHUNK_TYPE:
        e = &errorCauseUnrecognizedChunkType{}
    case PROTOCOL_VIOLATION:
        e = &errorCauseProtocolViolation{}
    default:
        return nil, fmt.Errorf("%w: %s", errBuildErrorCaseHandle, c.String())
    }

    if err := e.unmarshal(raw); err != nil {
        return nil, err
    }
    return e, nil
}*/

/// errorCauseCode is a cause code that appears in either a ERROR or ABORT chunk
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
        write!(f, "{}", s)
    }
}
