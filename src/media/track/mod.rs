pub mod track_local;
pub mod track_remote;

use track_remote::*;

use crate::media::interceptor::{stream_info::StreamInfo, *};
use std::sync::Arc;

/// trackStreams maintains a mapping of RTP/RTCP streams to a specific track
/// a RTPReceiver may contain multiple streams if we are dealing with Multicast
pub(crate) struct TrackStreams {
    track: TrackRemote,

    stream_info: StreamInfo,

    rtp_read_stream: Arc<srtp::stream::Stream>, //ReadStreamSRTP
    rtp_interceptor: Box<dyn RTPReader>,

    rtcp_read_stream: Arc<srtp::stream::Stream>, //ReadStreamSRTCP
    rtcp_interceptor: Box<dyn RTCPReader>,
}
