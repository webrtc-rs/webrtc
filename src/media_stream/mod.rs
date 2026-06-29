//! Media Stream Tracks
//!
//! This module provides the [`Track`] trait along with its specialized sub-traits [`TrackLocal`](crate::media_stream::track_local::TrackLocal)
//! and [`TrackRemote`](crate::media_stream::track_remote::TrackRemote), which represent WebRTC media tracks.
//!
//! # Concepts
//!
//! *   **[`Track`]**: The base trait for all media tracks, containing common metadata such as ID, kind (audio/video), SSRC, and codec.
//! *   **[`TrackLocal`](crate::media_stream::track_local::TrackLocal)**: A track representing media generated locally (e.g., from a microphone, camera, or file)
//!     to be sent to the remote peer.
//! *   **[`TrackRemote`](crate::media_stream::track_remote::TrackRemote)**: A track representing media received from the remote peer.
//!
//! # Implementations
//!
//! We provide two standard implementations of [`TrackLocal`](crate::media_stream::track_local::TrackLocal):
//! 1.  **[`TrackLocalStaticRTP`](crate::media_stream::track_local::static_rtp::TrackLocalStaticRTP)**: Used when the application already has pre-packetized RTP packets
//!     (e.g., from a media server or an RTP forwarder) and wants to write them directly.
//! 2.  **[`TrackLocalStaticSample`](crate::media_stream::track_local::static_sample::TrackLocalStaticSample)**: Used when the application has raw media samples
//!     (e.g., VP8, H.264, Opus frames) and wants the library to packetize and sequence them automatically.

/// Local media track implementations and traits.
pub mod track_local;
/// Remote media track implementations and traits.
pub mod track_remote;

pub use rtc::media_stream::{MediaStreamId, MediaStreamTrack, MediaStreamTrackId};
use rtc::media_stream::{
    MediaStreamTrackState, MediaTrackCapabilities, MediaTrackConstraints, MediaTrackSettings,
};
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpEncodingParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RtpStreamId, SSRC};

/// Represents a media track (either local or remote).
///
/// This trait provides an async-friendly abstraction for querying and configuring
/// WebRTC media tracks, including their streams, codecs, and parameters.
#[async_trait::async_trait]
pub trait Track: Send + Sync + 'static {
    /// Returns the ID of the media stream this track belongs to.
    async fn stream_id(&self) -> MediaStreamId;
    /// Returns the ID of the media track.
    async fn track_id(&self) -> MediaStreamTrackId;
    /// Returns the label of the media track.
    async fn label(&self) -> String;
    /// Returns the kind of media (Audio or Video) this track represents.
    async fn kind(&self) -> RtpCodecKind;
    /// Returns the RTP stream ID (RID) associated with the given SSRC, if any.
    async fn rid(&self, ssrc: SSRC) -> Option<RtpStreamId>;
    /// Returns the codec configured for the given SSRC, if any.
    async fn codec(&self, ssrc: SSRC) -> Option<RTCRtpCodec>;
    /// Returns the list of SSRCs associated with this track.
    async fn ssrcs(&self) -> Vec<SSRC>;
    /// Returns whether this track is enabled.
    async fn enabled(&self) -> bool;
    /// Enables or disables this track.
    async fn set_enabled(&self, enabled: bool);
    /// Returns whether this track is muted.
    async fn muted(&self) -> bool;
    /// Mutes or unmutes this track.
    async fn set_muted(&self, muted: bool);
    /// Returns the current state of the media track.
    async fn ready_state(&self) -> MediaStreamTrackState;
    /// Stops the media track.
    async fn stop(&self);
    /// Returns the capabilities of this media track.
    async fn get_capabilities(&self) -> MediaTrackCapabilities;
    /// Returns the constraints applied to this media track.
    async fn get_constraints(&self) -> MediaTrackConstraints;
    /// Returns the settings applied to this media track.
    async fn get_settings(&self) -> MediaTrackSettings;
    /// Applies constraints to this media track.
    async fn apply_constraints(&self, constraints: Option<MediaTrackConstraints>);
    /// Returns the RTP encoding parameters (codings) configured for this track.
    async fn codings(&self) -> Vec<RTCRtpEncodingParameters>;
    /// Adds an RTP encoding parameter (coding) to this track.
    async fn add_coding(&self, coding: RTCRtpEncodingParameters);
}
