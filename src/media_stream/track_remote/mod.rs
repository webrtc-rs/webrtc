use crate::media_stream::Track;
use rtc::{rtcp, rtp};

#[derive(Debug, Clone)]
pub enum TrackRemoteEvent {
    OnMute,
    OnUnmute,
    OnEnded,
    OnRtpPacket(rtp::Packet),
    OnRtcpPacket(Vec<Box<dyn rtcp::Packet>>),
}

#[async_trait::async_trait]
pub trait TrackRemote: Track {
    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> crate::error::Result<()>;
    async fn poll(&self) -> Option<TrackRemoteEvent>;
}
