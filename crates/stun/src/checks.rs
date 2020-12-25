use crate::attributes::*;
use crate::errors::*;

use util::Error;

use subtle::ConstantTimeEq;

// check_size returns ErrAttrSizeInvalid if got is not equal to expected.
pub fn check_size(_at: AttrType, got: usize, expected: usize) -> Result<(), Error> {
    if got == expected {
        Ok(())
    } else {
        Err(ERR_ATTRIBUTE_SIZE_INVALID.clone())
    }
}

// is_attr_size_invalid returns true if error means that attribute size is invalid.
pub fn is_attr_size_invalid(err: &Error) -> bool {
    *err == *ERR_ATTRIBUTE_SIZE_INVALID
}

pub(crate) fn check_hmac(got: &[u8], expected: &[u8]) -> Result<(), Error> {
    if got.ct_eq(expected).unwrap_u8() != 1 {
        Err(ERR_INTEGRITY_MISMATCH.clone())
    } else {
        Ok(())
    }
}

pub(crate) fn check_fingerprint(got: u32, expected: u32) -> Result<(), Error> {
    if got == expected {
        Ok(())
    } else {
        Err(ERR_FINGERPRINT_MISMATCH.clone())
    }
}

// check_overflow returns ErrAttributeSizeOverflow if got is bigger that max.
pub fn check_overflow(_at: AttrType, got: usize, max: usize) -> Result<(), Error> {
    if got <= max {
        Ok(())
    } else {
        Err(ERR_ATTRIBUTE_SIZE_OVERFLOW.clone())
    }
}

// is_attr_size_overflow returns true if error means that attribute size is too big.
pub fn is_attr_size_overflow(err: &Error) -> bool {
    *err == *ERR_ATTRIBUTE_SIZE_OVERFLOW
}
