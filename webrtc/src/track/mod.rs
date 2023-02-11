pub mod track_local;
pub mod track_remote;

use std::sync::Arc;

use interceptor::stream_info::StreamInfo;
use interceptor::{RTCPReader, RTPReader};
use track_remote::*;

pub(crate) const RTP_OUTBOUND_MTU: usize = 1200;
pub(crate) const RTP_PAYLOAD_TYPE_BITMASK: u8 = 0x7F;

#[derive(Clone)]
pub(crate) struct TrackStream {
    pub(crate) stream_info: Option<StreamInfo>,
    pub(crate) rtp_read_stream: Option<Arc<srtp::stream::Stream>>,
    pub(crate) rtp_interceptor: Option<Arc<dyn RTPReader + Send + Sync>>,
    pub(crate) rtcp_read_stream: Option<Arc<srtp::stream::Stream>>,
    pub(crate) rtcp_interceptor: Option<Arc<dyn RTCPReader + Send + Sync>>,
}

/// TrackStreams maintains a mapping of RTP/RTCP streams to a specific track
/// a RTPReceiver may contain multiple streams if we are dealing with Simulcast
#[derive(Clone)]
pub(crate) struct TrackStreams {
    pub(crate) track: Arc<TrackRemote>,
    pub(crate) stream: TrackStream,
    pub(crate) repair_stream: TrackStream,
}
