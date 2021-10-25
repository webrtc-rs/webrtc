pub mod track_local;
pub mod track_remote;

use track_remote::*;

use interceptor::stream_info::StreamInfo;
use interceptor::{RTCPReader, RTPReader};
use std::sync::Arc;

/// TrackStreams maintains a mapping of RTP/RTCP streams to a specific track
/// a RTPReceiver may contain multiple streams if we are dealing with Multicast
pub(crate) struct TrackStreams {
    pub(crate) track: Arc<TrackRemote>,

    pub(crate) stream_info: StreamInfo,

    pub(crate) rtp_read_stream: Option<Arc<srtp::stream::Stream>>, //ReadStreamSRTP
    pub(crate) rtp_interceptor: Option<Arc<dyn RTPReader + Send + Sync>>,
    pub(crate) rtcp_read_stream: Option<Arc<srtp::stream::Stream>>, //ReadStreamSRTCP
    pub(crate) rtcp_interceptor: Option<Arc<dyn RTCPReader + Send + Sync>>,
}
