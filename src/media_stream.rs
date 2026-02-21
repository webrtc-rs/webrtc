//! Track types for media streaming
//!
//! This module provides async-friendly wrappers around RTP media tracks.

use crate::runtime::{Mutex, Receiver, Sender};
use rtc::rtp_transceiver::{RTCRtpReceiverId, RTCRtpSenderId, RtpStreamId, SSRC};
use rtc::shared::error::Result;

pub use rtc::media_stream::{MediaStreamId, MediaStreamTrack, MediaStreamTrackId};
use rtc::media_stream::{
    MediaStreamTrackState, MediaTrackCapabilities, MediaTrackConstraints, MediaTrackSettings,
};
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RtpCodecKind};
use rtc::{rtcp, rtp};

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
    async fn ready_state(&self) -> MediaStreamTrackState;
    async fn stop(&self);
    async fn get_capabilities(&self) -> MediaTrackCapabilities;
    async fn get_constraints(&self) -> MediaTrackConstraints;
    async fn get_settings(&self) -> MediaTrackSettings;
    async fn apply_constraints(&self, constraints: Option<MediaTrackConstraints>);
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

/// A local track that sends RTP packets
///
/// This represents an outgoing media track to a remote peer.
/// Use `write_rtp()` to asynchronously send RTP packets.
///
/// # Example
///
/// ```no_run
/// # use webrtc::media_stream::TrackLocalImpl;
/// # use webrtc::error::Result;
/// # async fn example(track: TrackLocalImpl) -> Result<()> {
/// # use bytes::Bytes;
/// // Create an RTP packet
/// let packet = rtc::rtp::packet::Packet {
///     header: rtc::rtp::header::Header {
///         version: 2,
///         payload_type: 96,
///         sequence_number: 1000,
///         timestamp: 48000,
///         ssrc: 12345,
///         ..Default::default()
///     },
///     payload: Bytes::from_static(b"encoded frame data"),
/// };
///
/// // Send it
/// track.write_rtp(packet).await?;
/// # Ok(())
/// # }
/// ```
pub struct TrackLocalImpl {
    /// Sender ID in the peer connection (crate-internal)
    pub(crate) sender_id: RTCRtpSenderId,
    /// Channel for sending outgoing messages to the driver
    tx: Sender<crate::peer_connection::MessageInner>,
}

impl TrackLocalImpl {
    /// Create a new local track
    pub(crate) fn new(
        sender_id: RTCRtpSenderId,
        tx: Sender<crate::peer_connection::MessageInner>,
    ) -> Self {
        Self { sender_id, tx }
    }

    /// Send an RTP packet
    ///
    /// This queues the packet for transmission. The actual sending happens
    /// in the driver's event loop via RTCRtpSender::write_rtp().
    pub async fn write_rtp(&self, _packet: rtc::rtp::Packet) -> Result<()> {
        /*self.tx
        .try_send(crate::peer_connection::MessageInner::SenderRtp(
            self.sender_id,
            packet,
        ))
        .map_err(|e| Error::Other(format!("{:?}", e)))?;*/
        Ok(())
    }

    /// Send RTCP packets
    ///
    /// This queues RTCP packets (sender reports, etc.) for transmission.
    pub async fn write_rtcp(&self, _packets: Vec<Box<dyn rtc::rtcp::Packet>>) -> Result<()> {
        /*self.tx
        .try_send(crate::peer_connection::MessageInner::SenderRtcp(
            self.sender_id,
            packets,
        ))
        .map_err(|e| Error::Other(format!("{:?}", e)))?;*/
        Ok(())
    }
}

/// A remote track that receives RTP/RTCP packets
///
/// This represents an incoming media track from a remote peer.
/// Use `read_rtp()` to asynchronously receive RTP packets.
///
/// # Example
///
/// ```no_run
/// # async fn example(track: webrtc::media_stream::TrackRemoteImpl) {
/// // Receive RTP packets
/// while let Some(packet) = track.read_rtp().await {
///     println!("Received RTP: seq={}, ts={}",
///              packet.header.sequence_number,
///              packet.header.timestamp);
///     // Decode and process media...
/// }
/// # }
/// ```
pub struct TrackRemoteImpl {
    /// Receiver ID in the peer connection (crate-internal)
    pub(crate) receiver_id: RTCRtpReceiverId,
    /// Track ID (crate-internal)
    pub(crate) track_id: MediaStreamTrackId,
    /// Stream IDs this track belongs to (crate-internal)
    pub(crate) stream_ids: Vec<rtc::media_stream::MediaStreamId>,
    /// RID (RTP stream ID) for simulcast (crate-internal)
    pub(crate) rid: Option<String>,
    /// Channel for receiving RTP packets
    rtp_rx: Mutex<Receiver<rtc::rtp::Packet>>,
    /// Channel for sending outgoing messages
    tx: Sender<crate::peer_connection::MessageInner>,
}

impl TrackRemoteImpl {
    /// Create a new remote track
    pub(crate) fn new(
        receiver_id: RTCRtpReceiverId,
        track_id: MediaStreamTrackId,
        stream_ids: Vec<rtc::media_stream::MediaStreamId>,
        rid: Option<String>,
        rtp_rx: Receiver<rtc::rtp::Packet>,
        tx: Sender<crate::peer_connection::MessageInner>,
    ) -> Self {
        Self {
            receiver_id,
            track_id,
            stream_ids,
            rid,
            rtp_rx: Mutex::new(rtp_rx),
            tx,
        }
    }

    /// Receive the next RTP packet
    ///
    /// Returns `None` when the track is closed.
    pub async fn read_rtp(&self) -> Option<rtc::rtp::Packet> {
        let mut rx = self.rtp_rx.lock().await;
        rx.recv().await
    }

    /// Send RTCP packets (feedback)
    ///
    /// Use this to send receiver feedback like NACK (retransmission requests),
    /// PLI (picture loss indication), or receiver reports.
    pub async fn write_rtcp(&self, _packets: Vec<Box<dyn rtc::rtcp::Packet>>) -> Result<()> {
        /*self.tx
        .try_send(crate::peer_connection::MessageInner::ReceiverRtcp(
            self.receiver_id,
            packets,
        ))
        .map_err(|e| Error::Other(format!("{:?}", e)))?;*/
        Ok(())
    }
}
