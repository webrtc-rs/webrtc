use async_trait::async_trait;
use srtp::stream::Stream;

use crate::error::Result;
use crate::{Attributes, RTCPReader, RTPReader};

#[async_trait]
impl RTPReader for Stream {
    async fn read(
        &self,
        buf: &mut [u8],
        a: &Attributes,
    ) -> Result<(rtp::packet::Packet, Attributes)> {
        Ok((self.read_rtp(buf).await?, a.clone()))
    }
}

#[async_trait]
impl RTCPReader for Stream {
    async fn read(
        &self,
        buf: &mut [u8],
        a: &Attributes,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        Ok((self.read_rtcp(buf).await?, a.clone()))
    }
}
