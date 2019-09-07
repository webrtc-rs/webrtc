use crate::session::Session;

use util::Error;

pub mod stream_srtcp;
pub mod stream_srtp;

pub trait ReadStream {
    fn init(&self, child: Session, ssrc: u32) -> Result<(), Error>;
    fn read(&self, buf: &[u8]) -> Result<isize, Error>;
    fn get_ssrc(&self) -> u32;
}
