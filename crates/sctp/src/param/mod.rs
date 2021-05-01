use crate::error::Error;

use bytes::Bytes;

pub(crate) trait Param {
    fn marshal(&self) -> Result<Bytes, Error>;
    fn length(&self) -> usize;
}
