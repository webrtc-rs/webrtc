use crate::error::Result;
use crate::media_stream::Track;
use crate::peer_connection::MessageInner;
use crate::runtime::Sender;
use rtc::rtp_transceiver::RTCRtpSenderId;
use rtc::{rtcp, rtp};

pub mod static_rtp;

/// TrackLocalContext is the Context passed when a TrackLocal has been Binded/Unbinded from a PeerConnection, and used
/// in Interceptors.
#[derive(Clone)]
pub struct TrackLocalContext {
    pub(crate) sender_id: RTCRtpSenderId,
    pub(crate) msg_tx: Sender<MessageInner>,
}

#[async_trait::async_trait]
pub trait TrackLocal: Track {
    /// bind should implement the way how the media data flows from the Track to the PeerConnection
    /// This will be called internally after signaling is complete and the list of available
    /// codecs has been determined
    async fn bind(&self, ctx: TrackLocalContext);

    /// unbind should implement the teardown logic when the track is no longer needed. This happens
    /// because a track has been stopped.
    async fn unbind(&self);

    async fn write_rtp(&self, packet: rtp::Packet) -> Result<()>;

    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> Result<()>;
}
