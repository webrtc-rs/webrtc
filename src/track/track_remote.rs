//! Remote track for receiving media

use rtc::media_stream::MediaStreamTrackId;
use rtc::rtp::packet::Packet as RtpPacket;
use rtc::rtp_transceiver::RTCRtpReceiverId;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::mpsc;

/// Message for outgoing RTCP packets from receiver (crate-internal)
#[derive(Debug)]
pub(crate) struct OutgoingReceiverRtcpPackets {
    pub(crate) receiver_id: RTCRtpReceiverId,
    pub(crate) packets: Vec<Box<dyn rtc::rtcp::Packet + Send>>,
}

/// A remote track that receives RTP/RTCP packets
///
/// This represents an incoming media track from a remote peer.
/// Use `read_rtp()` to asynchronously receive RTP packets.
///
/// # Example
///
/// ```no_run
/// # async fn example(track: webrtc::track::TrackRemote) {
/// // Receive RTP packets
/// while let Some(packet) = track.read_rtp().await {
///     println!("Received RTP: seq={}, ts={}",
///              packet.header.sequence_number,
///              packet.header.timestamp);
///     // Decode and process media...
/// }
/// # }
/// ```
pub struct TrackRemote {
    /// Receiver ID in the peer connection (crate-internal)
    pub(crate) receiver_id: RTCRtpReceiverId,
    /// Track ID (crate-internal)
    pub(crate) track_id: MediaStreamTrackId,
    /// Stream IDs this track belongs to (crate-internal)
    pub(crate) stream_ids: Vec<rtc::media_stream::MediaStreamId>,
    /// RID (RTP stream ID) for simulcast (crate-internal)
    pub(crate) rid: Option<String>,
    /// Channel for receiving RTP packets
    rtp_rx: AsyncMutex<mpsc::UnboundedReceiver<RtpPacket>>,
    /// Channel for sending RTCP packets (feedback)
    rtcp_tx: mpsc::UnboundedSender<OutgoingReceiverRtcpPackets>,
}

impl TrackRemote {
    /// Create a new remote track
    pub(crate) fn new(
        receiver_id: RTCRtpReceiverId,
        track_id: MediaStreamTrackId,
        stream_ids: Vec<rtc::media_stream::MediaStreamId>,
        rid: Option<String>,
        rtp_rx: mpsc::UnboundedReceiver<RtpPacket>,
        rtcp_tx: mpsc::UnboundedSender<OutgoingReceiverRtcpPackets>,
    ) -> Self {
        Self {
            receiver_id,
            track_id,
            stream_ids,
            rid,
            rtp_rx: AsyncMutex::new(rtp_rx),
            rtcp_tx,
        }
    }

    /// Receive the next RTP packet
    ///
    /// Returns `None` when the track is closed.
    pub async fn read_rtp(&self) -> Option<RtpPacket> {
        let mut rx = self.rtp_rx.lock().await;
        rx.recv().await
    }

    /// Send RTCP packets (feedback)
    ///
    /// Use this to send receiver feedback like NACK (retransmission requests),
    /// PLI (picture loss indication), or receiver reports.
    pub async fn write_rtcp(
        &self,
        packets: Vec<Box<dyn rtc::rtcp::Packet + Send>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.rtcp_tx
            .send(OutgoingReceiverRtcpPackets {
                receiver_id: self.receiver_id,
                packets,
            })
            .map_err(|e| format!("Failed to send RTCP packets: {}", e))?;
        Ok(())
    }
}
