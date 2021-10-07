use crate::error::Result;
use crate::{Attributes, RTCPReader, RTPReader};

use async_trait::async_trait;
use srtp::stream::Stream;

#[async_trait]
impl RTPReader for Stream {
    async fn read(&self, buf: &mut [u8], a: &Attributes) -> Result<(usize, Attributes)> {
        Ok((self.read(buf).await?, a.clone()))
    }
}

#[async_trait]
impl RTCPReader for Stream {
    async fn read(&self, buf: &mut [u8], a: &Attributes) -> Result<(usize, Attributes)> {
        Ok((self.read(buf).await?, a.clone()))
    }
}
