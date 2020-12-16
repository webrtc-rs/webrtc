use crate::attributes::*;
use crate::errors::*;

use util::Error;

// check_size returns ErrAttrSizeInvalid if got is not equal to expected.
pub fn check_size(_at: AttrType, got: usize, expected: usize) -> Result<(), Error> {
    if got == expected {
        Ok(())
    } else {
        Err(ERR_ATTRIBUTE_SIZE_INVALID.clone())
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
