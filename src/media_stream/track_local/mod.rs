//! Local Media Stream Tracks
//!
//! This module provides the [`TrackLocal`](crate::media_stream::track_local::TrackLocal) trait, which represents a media track generated locally.
//! It also includes two concrete implementations:
//! *   **[`TrackLocalStaticRTP`](crate::media_stream::track_local::static_rtp::TrackLocalStaticRTP)**: For writing pre-packetized RTP packets.
//! *   **[`TrackLocalStaticSample`](crate::media_stream::track_local::static_sample::TrackLocalStaticSample)**: For writing raw media samples.
//!
//! # Examples
//!
//! ## Writing Media Samples
//!
//! ```no_run
//! use webrtc::media_stream::track_local::TrackLocal;
//! use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
//! use rtc::media_stream::{MediaStreamTrack, MediaStreamTrackId, MediaStreamId};
//! use rtc::rtp_transceiver::rtp_sender::RtpCodecKind;
//! use rtc::media::Sample;
//! use std::time::Duration;
//! use std::sync::Arc;
//!
//! # async fn example() -> webrtc::error::Result<()> {
//! // Create a local video track
//! let track = MediaStreamTrack::new(
//!     MediaStreamTrackId::new(),
//!     MediaStreamId::new(),
//!     "video-label".to_owned(),
//!     RtpCodecKind::Video,
//!     vec![],
//! );
//! let local_track = Arc::new(TrackLocalStaticSample::new(track)?);
//!
//! // Write a raw VP8/H.264 frame as a sample
//! let sample = Sample {
//!     data: bytes::Bytes::from(vec![0x00, 0x01, 0x02]),
//!     duration: Duration::from_millis(33), // ~30 fps
//!     ..Default::default()
//! };
//!
//! // Write the sample to SSRC 1234 and PT 96
//! local_track.write_sample(1234, 96, &sample, &[]).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::Result;
use crate::media_stream::Track;
use crate::peer_connection::driver::PeerConnectionDriverEvent;
use crate::runtime::{Receiver, Sender};
use rtc::media_stream::MediaStreamTrack;
use rtc::rtp_transceiver::RTCRtpSenderId;
use rtc::rtp_transceiver::rtp_sender::RTCRtpParameters;
use rtc::{rtcp, rtp};

/// Events that can occur on a [`TrackLocal`] (a track we send).
#[derive(Debug, Clone)]
pub enum TrackLocalEvent {
    /// Fired when RTCP feedback about this sent track is received from the remote peer —
    /// e.g. Receiver Reports, or PLI/FIR keyframe requests. An SFU relays such feedback
    /// upstream to the publisher.
    OnRtcpPacket(Vec<Box<dyn rtcp::Packet>>),
}

/// Local track implementation that accepts pre-packetized RTP packets.
pub mod static_rtp;
/// Local track implementation that accepts raw media samples and packetizes them.
pub mod static_sample;

/// TrackLocalContext is the Context passed when a TrackLocal has been Binded/Unbinded from a PeerConnection, and used
/// in Interceptors.
#[derive(Clone)]
pub struct TrackLocalContext {
    pub(crate) rtp_sender_id: RTCRtpSenderId,
    pub(crate) rtp_parameters: RTCRtpParameters,
    pub(crate) driver_event_tx: Sender<PeerConnectionDriverEvent>,
}

/// A local media track that can be sent to a remote peer.
///
/// This trait defines the interface for local media tracks. Applications write
/// RTP and RTCP packets to this track, which are then processed by the interceptor
/// pipeline and sent over the peer connection.
#[async_trait::async_trait]
pub trait TrackLocal: Track {
    /// Returns the underlying [`MediaStreamTrack`] for this local track.
    async fn track(&self) -> MediaStreamTrack;

    /// Binds the track to the peer connection context.
    ///
    /// This will be called internally after signaling is complete and the list of available
    /// codecs has been determined. `evt_rx` delivers events for this track (see
    /// [`TrackLocal::poll`]).
    async fn bind(&self, ctx: TrackLocalContext, evt_rx: Receiver<TrackLocalEvent>);

    /// Unbinds the track from the peer connection context, cleaning up any resources.
    async fn unbind(&self);

    /// Writes an RTP packet to the track.
    async fn write_rtp(&self, packet: rtp::Packet) -> Result<()>;

    /// Writes RTCP packets to the track.
    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> Result<()>;

    /// Polls for the next event on the local track (RTCP feedback from the remote peer).
    /// Returns `None` once the track is unbound / the peer connection closes.
    async fn poll(&self) -> Option<TrackLocalEvent>;
}
