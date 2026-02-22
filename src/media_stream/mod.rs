//! Track types for media streaming
//!
//! This module provides async-friendly wrappers around RTP media tracks.

mod track_local;
mod track_remote;

use rtc::shared::error::Result;
use rtc::{rtcp, rtp};

pub use rtc::media_stream::{MediaStreamId, MediaStreamTrack, MediaStreamTrackId};

pub trait Track: Send + Sync + 'static {
    fn track(&self) -> &MediaStreamTrack;
}

#[async_trait::async_trait]
pub trait TrackLocal: Track {
    async fn write_rtp(&self, packet: rtp::Packet) -> Result<()>;
    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> Result<()>;
}

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
    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> Result<()>;
    async fn poll(&self) -> Option<TrackRemoteEvent>;
}
