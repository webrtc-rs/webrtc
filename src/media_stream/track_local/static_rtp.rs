use crate::media_stream::Track;
use crate::media_stream::track_local::TrackLocal;
use rtc::media_stream::MediaStreamTrack;
use rtc::rtp::Packet;

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Debug)]
pub struct TrackLocalStaticRTP {
    track: MediaStreamTrack,
}

impl TrackLocalStaticRTP {
    pub fn new(track: MediaStreamTrack) -> Self {
        Self { track }
    }
}

impl Track for TrackLocalStaticRTP {
    fn track(&self) -> &MediaStreamTrack {
        &self.track
    }
}

#[async_trait::async_trait]
impl TrackLocal for TrackLocalStaticRTP {
    async fn write_rtp(&self, _packet: Packet) -> crate::error::Result<()> {
        todo!()
    }

    async fn write_rtcp(
        &self,
        _packets: Vec<Box<dyn rtc::rtcp::Packet>>,
    ) -> crate::error::Result<()> {
        todo!()
    }
}
