//! Track types for media streaming
//!
//! This module provides async-friendly wrappers around RTP media tracks.

pub mod track_local;
pub mod track_remote;

pub use rtc::media_stream::{MediaStreamId, MediaStreamTrack, MediaStreamTrackId};

pub trait Track: Send + Sync + 'static {
    fn track(&self) -> &MediaStreamTrack;
}
