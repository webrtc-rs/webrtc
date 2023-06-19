use std::collections::HashMap;
use std::fmt;

use crate::attributes::*;
use crate::checks::*;
use crate::error::*;
use crate::message::*;

// ErrorCodeAttribute represents ERROR-CODE attribute.
//
// RFC 5389 Section 15.6
#[derive(Default)]
pub struct ErrorCodeAttribute {
    pub code: ErrorCode,
    pub reason: Vec<u8>,
}

impl fmt::Display for ErrorCodeAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match String::from_utf8(self.reason.clone()) {
            Ok(reason) => reason,
            Err(_) => return Err(fmt::Error {}),
        };

        write!(f, "{}: {}", self.code.0, reason)
    }
}

// constants for ERROR-CODE encoding.
const ERROR_CODE_CLASS_BYTE: usize = 2;
const ERROR_CODE_NUMBER_BYTE: usize = 3;
const ERROR_CODE_REASON_START: usize = 4;
const ERROR_CODE_REASON_MAX_B: usize = 763;
const ERROR_CODE_MODULO: u16 = 100;

impl Setter for ErrorCodeAttribute {
    // add_to adds ERROR-CODE to m.
    fn add_to(&self, m: &mut Message) -> Result<()> {
        check_overflow(
            ATTR_ERROR_CODE,
            self.reason.len() + ERROR_CODE_REASON_START,
            ERROR_CODE_REASON_MAX_B + ERROR_CODE_REASON_START,
        )?;

        let mut value: Vec<u8> = Vec::with_capacity(ERROR_CODE_REASON_MAX_B);

        let number = (self.code.0 % ERROR_CODE_MODULO) as u8; // error code modulo 100
        let class = (self.code.0 / ERROR_CODE_MODULO) as u8; // hundred digit
        value.extend_from_slice(&[0, 0]);
        value.push(class); // [ERROR_CODE_CLASS_BYTE]
        value.push(number); //[ERROR_CODE_NUMBER_BYTE] =
        value.extend_from_slice(&self.reason); //[ERROR_CODE_REASON_START:]

        m.add(ATTR_ERROR_CODE, &value);

        Ok(())
    }
}

impl Getter for ErrorCodeAttribute {
    // GetFrom decodes ERROR-CODE from m. Reason is valid until m.Raw is valid.
    fn get_from(&mut self, m: &Message) -> Result<()> {
        let v = m.get(ATTR_ERROR_CODE)?;

        if v.len() < ERROR_CODE_REASON_START {
            return Err(Error::ErrUnexpectedEof);
        }

        let class = v[ERROR_CODE_CLASS_BYTE] as u16;
        let number = v[ERROR_CODE_NUMBER_BYTE] as u16;
        let code = class * ERROR_CODE_MODULO + number;
        self.code = ErrorCode(code);
        self.reason = v[ERROR_CODE_REASON_START..].to_vec();

        Ok(())
    }
}

// ErrorCode is code for ERROR-CODE attribute.
#[derive(PartialEq, Eq, Hash, Copy, Clone, Default)]
pub struct ErrorCode(pub u16);

impl Setter for ErrorCode {
    // add_to adds ERROR-CODE with default reason to m. If there
    // is no default reason, returns ErrNoDefaultReason.
    fn add_to(&self, m: &mut Message) -> Result<()> {
        if let Some(reason) = ERROR_REASONS.get(self) {
            let a = ErrorCodeAttribute {
                code: *self,
                reason: reason.clone(),
            };
            a.add_to(m)
        } else {
            Err(Error::ErrNoDefaultReason)
        }
    }
}

// Possible error codes.
pub const CODE_TRY_ALTERNATE: ErrorCode = ErrorCode(300);
pub const CODE_BAD_REQUEST: ErrorCode = ErrorCode(400);
pub const CODE_UNAUTHORIZED: ErrorCode = ErrorCode(401);
pub const CODE_UNKNOWN_ATTRIBUTE: ErrorCode = ErrorCode(420);
pub const CODE_STALE_NONCE: ErrorCode = ErrorCode(438);
pub const CODE_ROLE_CONFLICT: ErrorCode = ErrorCode(487);
pub const CODE_SERVER_ERROR: ErrorCode = ErrorCode(500);

// DEPRECATED constants.
// DEPRECATED, use CODE_UNAUTHORIZED.
pub const CODE_UNAUTHORISED: ErrorCode = CODE_UNAUTHORIZED;

// Error codes from RFC 5766.
//
// RFC 5766 Section 15
pub const CODE_FORBIDDEN: ErrorCode = ErrorCode(403); // Forbidden
pub const CODE_ALLOC_MISMATCH: ErrorCode = ErrorCode(437); // Allocation Mismatch
pub const CODE_WRONG_CREDENTIALS: ErrorCode = ErrorCode(441); // Wrong Credentials
pub const CODE_UNSUPPORTED_TRANS_PROTO: ErrorCode = ErrorCode(442); // Unsupported Transport Protocol
pub const CODE_ALLOC_QUOTA_REACHED: ErrorCode = ErrorCode(486); // Allocation Quota Reached
pub const CODE_INSUFFICIENT_CAPACITY: ErrorCode = ErrorCode(508); // Insufficient Capacity

// Error codes from RFC 6062.
//
// RFC 6062 Section 6.3
pub const CODE_CONN_ALREADY_EXISTS: ErrorCode = ErrorCode(446);
pub const CODE_CONN_TIMEOUT_OR_FAILURE: ErrorCode = ErrorCode(447);

// Error codes from RFC 6156.
//
// RFC 6156 Section 10.2
pub const CODE_ADDR_FAMILY_NOT_SUPPORTED: ErrorCode = ErrorCode(440); // Address Family not Supported
pub const CODE_PEER_ADDR_FAMILY_MISMATCH: ErrorCode = ErrorCode(443); // Peer Address Family Mismatch

lazy_static! {
    pub static ref ERROR_REASONS:HashMap<ErrorCode, Vec<u8>> =
        [
            (CODE_TRY_ALTERNATE,     b"Try Alternate".to_vec()),
            (CODE_BAD_REQUEST,       b"Bad Request".to_vec()),
            (CODE_UNAUTHORIZED,     b"Unauthorized".to_vec()),
            (CODE_UNKNOWN_ATTRIBUTE, b"Unknown Attribute".to_vec()),
            (CODE_STALE_NONCE,       b"Stale Nonce".to_vec()),
            (CODE_SERVER_ERROR,      b"Server Error".to_vec()),
            (CODE_ROLE_CONFLICT,     b"Role Conflict".to_vec()),

            // RFC 5766.
            (CODE_FORBIDDEN,             b"Forbidden".to_vec()),
            (CODE_ALLOC_MISMATCH,         b"Allocation Mismatch".to_vec()),
            (CODE_WRONG_CREDENTIALS,      b"Wrong Credentials".to_vec()),
            (CODE_UNSUPPORTED_TRANS_PROTO, b"Unsupported Transport Protocol".to_vec()),
            (CODE_ALLOC_QUOTA_REACHED,     b"Allocation Quota Reached".to_vec()),
            (CODE_INSUFFICIENT_CAPACITY,  b"Insufficient Capacity".to_vec()),

            // RFC 6062.
            (CODE_CONN_ALREADY_EXISTS,    b"Connection Already Exists".to_vec()),
            (CODE_CONN_TIMEOUT_OR_FAILURE, b"Connection Timeout or Failure".to_vec()),

            // RFC 6156.
            (CODE_ADDR_FAMILY_NOT_SUPPORTED, b"Address Family not Supported".to_vec()),
            (CODE_PEER_ADDR_FAMILY_MISMATCH, b"Peer Address Family Mismatch".to_vec()),
        ].iter().cloned().collect();

}
