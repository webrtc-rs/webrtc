pub(crate) mod static_rtp;

use crate::media_stream::Track;
use rtc::{rtcp, rtp};

#[derive(Debug, Clone)]
pub enum TrackRemoteEvent {
    OnOpen,
    OnError,
    OnEnding, // RTCTrackEvent::OnClosing
    OnEnded,  // RTCTrackEvent::OnClose

    OnMute,
    OnUnmute,

    OnRtpPacket(rtp::Packet),
    OnRtcpPacket(Vec<Box<dyn rtcp::Packet>>),
}

#[async_trait::async_trait]
pub trait TrackRemote: Track {
    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> crate::error::Result<()>;
    async fn poll(&self) -> Option<TrackRemoteEvent>;
}
