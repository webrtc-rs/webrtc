use crate::session::StreamSession;

use util::Error;

use async_trait::async_trait;

pub mod stream_srtcp;
pub mod stream_srtp;

#[async_trait]
pub trait ReadStream: Send {
    async fn init(&mut self, ssrc: u32) -> Result<(), Error>;
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error>;
    fn get_ssrc(&mut self) -> u32;
}
