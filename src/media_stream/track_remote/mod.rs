//! Remote Media Stream Tracks
//!
//! This module provides the [`TrackRemote`](crate::media_stream::track_remote::TrackRemote) trait and the [`TrackRemoteEvent`](crate::media_stream::track_remote::TrackRemoteEvent) enum, which represent
//! a media track received from a remote peer.
//!
//! # Concepts
//!
//! *   **[`TrackRemote`](crate::media_stream::track_remote::TrackRemote)**: A track representing incoming media. It is received via the
//!     [`PeerConnectionEventHandler::on_track`](crate::peer_connection::PeerConnectionEventHandler::on_track) callback.
//! *   **Event Polling**: Incoming RTP packets, RTCP packets, and track lifecycle events (such as mute, unmute,
//!     or closing) are fetched by calling the asynchronous [`TrackRemote::poll`](crate::media_stream::track_remote::TrackRemote::poll) method in a loop.
//!
//! # Examples
//!
//! ## Reading Incoming RTP Packets
//!
//! ```no_run
//! # use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
//! # use std::sync::Arc;
//! # async fn handle_remote_track(track: Arc<dyn TrackRemote>) {
//! while let Some(event) = track.poll().await {
//!     match event {
//!         TrackRemoteEvent::OnRtpPacket(pkt) => {
//!             println!("Received RTP packet: sequence_number={}", pkt.header.sequence_number);
//!             // Process/decode the RTP payload
//!         }
//!         TrackRemoteEvent::OnEnded => {
//!             println!("Remote track ended");
//!             break;
//!         }
//!         _ => {}
//!     }
//! }
//! # }
//! ```

pub(crate) mod static_rtp;

use crate::error::Result;
use crate::media_stream::Track;
use rtc::peer_connection::event::RTCTrackEventInit;
use rtc::{rtcp, rtp};

/// Events that can occur on a [`TrackRemote`].
#[derive(Debug, Clone)]
pub enum TrackRemoteEvent {
    /// Fired when the track is opened and ready to receive packets.
    OnOpen(RTCTrackEventInit),
    /// Fired when a track error occurs.
    OnError,
    /// Fired when the track is ending (closing).
    OnEnding, // RTCTrackEvent::OnClosing
    /// Fired when the track has ended.
    OnEnded, // RTCTrackEvent::OnClose

    /// Fired when the track is muted.
    OnMute,
    /// Fired when the track is unmuted.
    OnUnmute,

    /// Fired when a new RTP packet is received on this track.
    OnRtpPacket(rtp::Packet),
    /// Fired when new RTCP packets are received on this track.
    OnRtcpPacket(Vec<Box<dyn rtcp::Packet>>),
}

/// A remote media track received from a remote peer.
///
/// This trait provides the interface for reading incoming RTP/RTCP packets
/// and sending RTCP feedback (such as PLI, SLI, or NACK) back to the sender.
#[async_trait::async_trait]
pub trait TrackRemote: Track {
    /// Writes RTCP feedback packets to the remote track sender.
    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> Result<()>;

    /// Polls for the next event on the remote track.
    async fn poll(&self) -> Option<TrackRemoteEvent>;
}
