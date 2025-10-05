use async_trait::async_trait;
use srtp::stream::Stream;

use crate::error::Result;
use crate::{RTCPReader, RTPReader};

#[async_trait]
impl RTPReader for Stream {
    async fn read(&self, buf: &mut [u8]) -> Result<rtp::packet::Packet> {
        Ok(self.read_rtp(buf).await?)
    }
}

#[async_trait]
impl RTCPReader for Stream {
    async fn read(
        &self,
        buf: &mut [u8],
    ) -> Result<Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>> {
        Ok(self.read_rtcp(buf).await?)
    }
}
