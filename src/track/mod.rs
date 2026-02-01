//! Track types for media streaming
//!
//! This module provides async-friendly wrappers around RTP media tracks.

mod track_local;
mod track_remote;

pub use track_local::TrackLocal;
pub use track_remote::TrackRemote;

// Internal message types
pub(crate) use track_local::{OutgoingRtcpPackets, OutgoingRtpPacket};
pub(crate) use track_remote::OutgoingReceiverRtcpPackets;

// Re-export common types from rtc
pub use rtc::media_stream::{MediaStreamId, MediaStreamTrack, MediaStreamTrackId};
pub use rtc::rtp_transceiver::{
    RTCRtpReceiverId, RTCRtpSenderId, RTCRtpTransceiverDirection, RTCRtpTransceiverInit,
};
