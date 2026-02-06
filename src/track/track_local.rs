//! Local track for sending media

use crate::runtime::Sender;
use rtc::rtp::packet::Packet as RtpPacket;
use rtc::rtp_transceiver::RTCRtpSenderId;

/// A local track that sends RTP packets
///
/// This represents an outgoing media track to a remote peer.
/// Use `write_rtp()` to asynchronously send RTP packets.
///
/// # Example
///
/// ```no_run
/// # use webrtc::track::TrackLocal;
/// # async fn example(track: std::sync::Arc<TrackLocal>) -> Result<(), Box<dyn std::error::Error>> {
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
pub struct TrackLocal {
    /// Sender ID in the peer connection (crate-internal)
    pub(crate) sender_id: RTCRtpSenderId,
    /// Channel for sending outgoing messages to the driver
    tx: Sender<crate::peer_connection::InnerMessage>,
}

impl TrackLocal {
    /// Create a new local track
    pub(crate) fn new(
        sender_id: RTCRtpSenderId,
        tx: Sender<crate::peer_connection::InnerMessage>,
    ) -> Self {
        Self { sender_id, tx }
    }

    /// Send an RTP packet
    ///
    /// This queues the packet for transmission. The actual sending happens
    /// in the driver's event loop via RTCRtpSender::write_rtp().
    pub async fn write_rtp(&self, packet: RtpPacket) -> Result<(), Box<dyn std::error::Error>> {
        self.tx
            .try_send(crate::peer_connection::InnerMessage::SenderRtp(
                self.sender_id,
                packet,
            ))
            .map_err(|e| format!("Failed to send RTP packet: {:?}", e))?;
        Ok(())
    }

    /// Send RTCP packets
    ///
    /// This queues RTCP packets (sender reports, etc.) for transmission.
    pub async fn write_rtcp(
        &self,
        packets: Vec<Box<dyn rtc::rtcp::Packet>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.tx
            .try_send(crate::peer_connection::InnerMessage::SenderRtcp(
                self.sender_id,
                packets,
            ))
            .map_err(|e| format!("Failed to send RTCP packets: {:?}", e))?;
        Ok(())
    }
}
