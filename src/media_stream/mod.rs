//! Track types for media streaming
//!
//! This module provides async-friendly wrappers around RTP media tracks.

pub mod track_local;
pub mod track_remote;

pub use rtc::media_stream::{MediaStreamId, MediaStreamTrack, MediaStreamTrackId};
use rtc::media_stream::{
    MediaStreamTrackState, MediaTrackCapabilities, MediaTrackConstraints, MediaTrackSettings,
};
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpEncodingParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RtpStreamId, SSRC};

#[async_trait::async_trait]
pub trait Track: Send + Sync + 'static {
    async fn stream_id(&self) -> MediaStreamId;
    async fn track_id(&self) -> MediaStreamTrackId;
    async fn label(&self) -> String;
    async fn kind(&self) -> RtpCodecKind;
    async fn rid(&self, ssrc: SSRC) -> Option<RtpStreamId>;
    async fn codec(&self, ssrc: SSRC) -> Option<RTCRtpCodec>;
    async fn ssrcs(&self) -> Vec<SSRC>;
    async fn enabled(&self) -> bool;
    async fn set_enabled(&self, enabled: bool);
    async fn muted(&self) -> bool;
    async fn set_muted(&self, muted: bool);
    async fn ready_state(&self) -> MediaStreamTrackState;
    async fn stop(&self);
    async fn get_capabilities(&self) -> MediaTrackCapabilities;
    async fn get_constraints(&self) -> MediaTrackConstraints;
    async fn get_settings(&self) -> MediaTrackSettings;
    async fn apply_constraints(&self, constraints: Option<MediaTrackConstraints>);
    async fn codings(&self) -> Vec<RTCRtpEncodingParameters>;
    async fn add_coding(&self, coding: RTCRtpEncodingParameters);
}
