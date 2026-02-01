//! Local track for sending media

use rtc::rtp::packet::Packet as RtpPacket;
use rtc::rtp_transceiver::RTCRtpSenderId;
use tokio::sync::mpsc;

/// Message for outgoing RTP packets (crate-internal)
#[derive(Debug)]
pub(crate) struct OutgoingRtpPacket {
    pub(crate) sender_id: RTCRtpSenderId,
    pub(crate) packet: RtpPacket,
}

/// Message for outgoing RTCP packets (crate-internal)
#[derive(Debug)]
pub(crate) struct OutgoingRtcpPackets {
    pub(crate) sender_id: RTCRtpSenderId,
    pub(crate) packets: Vec<Box<dyn rtc::rtcp::Packet + Send>>,
}

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
    /// Channel for sending RTP packets to the driver
    rtp_tx: mpsc::UnboundedSender<OutgoingRtpPacket>,
    /// Channel for sending RTCP packets to the driver
    rtcp_tx: mpsc::UnboundedSender<OutgoingRtcpPackets>,
}

impl TrackLocal {
    /// Create a new local track
    pub(crate) fn new(
        sender_id: RTCRtpSenderId,
        rtp_tx: mpsc::UnboundedSender<OutgoingRtpPacket>,
        rtcp_tx: mpsc::UnboundedSender<OutgoingRtcpPackets>,
    ) -> Self {
        Self {
            sender_id,
            rtp_tx,
            rtcp_tx,
        }
    }

    /// Send an RTP packet
    ///
    /// This queues the packet for transmission. The actual sending happens
    /// in the driver's event loop via RTCRtpSender::write_rtp().
    pub async fn write_rtp(&self, packet: RtpPacket) -> Result<(), Box<dyn std::error::Error>> {
        self.rtp_tx
            .send(OutgoingRtpPacket {
                sender_id: self.sender_id,
                packet,
            })
            .map_err(|e| format!("Failed to send RTP packet: {}", e))?;
        Ok(())
    }

    /// Send RTCP packets
    ///
    /// This queues RTCP packets (sender reports, etc.) for transmission.
    pub async fn write_rtcp(
        &self,
        packets: Vec<Box<dyn rtc::rtcp::Packet + Send>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.rtcp_tx
            .send(OutgoingRtcpPackets {
                sender_id: self.sender_id,
                packets,
            })
            .map_err(|e| format!("Failed to send RTCP packets: {}", e))?;
        Ok(())
    }
}
