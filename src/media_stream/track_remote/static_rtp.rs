use crate::media_stream::Track;
use crate::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use crate::runtime::{Mutex, Receiver};
use rtc::media_stream::MediaStreamTrack;

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
pub(crate) struct TrackRemoteStaticRTP {
    track: MediaStreamTrack,
    evt_rx: Mutex<Receiver<TrackRemoteEvent>>,
}

impl TrackRemoteStaticRTP {
    pub fn new(track: MediaStreamTrack, evt_rx: Receiver<TrackRemoteEvent>) -> Self {
        Self {
            track,
            evt_rx: Mutex::new(evt_rx),
        }
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
        self.evt_rx.lock().await.recv().await
    }
}
