use crate::media_stream::Track;
use crate::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use rtc::media_stream::MediaStreamTrack;

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Debug)]
pub struct TrackRemoteStaticRTP {
    track: MediaStreamTrack,
}

impl TrackRemoteStaticRTP {
    pub fn new(track: MediaStreamTrack) -> Self {
        Self { track }
    }
}

impl Track for TrackRemoteStaticRTP {
    fn track(&self) -> &MediaStreamTrack {
        &self.track
    }
}

#[async_trait::async_trait]
impl TrackRemote for TrackRemoteStaticRTP {
    async fn write_rtcp(
        &self,
        _packets: Vec<Box<dyn rtc::rtcp::Packet>>,
    ) -> crate::error::Result<()> {
        todo!()
    }

    async fn poll(&self) -> Option<TrackRemoteEvent> {
        todo!()
    }
}
