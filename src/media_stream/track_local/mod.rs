use crate::media_stream::Track;
use rtc::{rtcp, rtp};

pub mod static_rtp;

#[async_trait::async_trait]
pub trait TrackLocal: Track {
    async fn write_rtp(&self, packet: rtp::Packet) -> crate::error::Result<()>;
    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> crate::error::Result<()>;
}
