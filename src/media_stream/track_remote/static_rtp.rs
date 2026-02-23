use crate::error::{Error, Result};
use crate::media_stream::Track;
use crate::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use crate::peer_connection::MessageInner;
use crate::runtime::{Mutex, Receiver, Sender};
use rtc::media_stream::MediaStreamTrack;
use rtc::rtp_transceiver::RTCRtpReceiverId;

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Clone)]
pub(crate) struct TrackRemoteStaticRTP {
    track: MediaStreamTrack,
    receiver_id: RTCRtpReceiverId,
    msg_tx: Sender<MessageInner>,
    evt_rx: Mutex<Receiver<TrackRemoteEvent>>,
}

impl TrackRemoteStaticRTP {
    pub fn new(
        track: MediaStreamTrack,
        receiver_id: RTCRtpReceiverId,
        msg_tx: Sender<MessageInner>,
        evt_rx: Receiver<TrackRemoteEvent>,
    ) -> Self {
        Self {
            track,
            receiver_id,
            msg_tx,
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
    async fn write_rtcp(&self, packets: Vec<Box<dyn rtc::rtcp::Packet>>) -> Result<()> {
        self.msg_tx
            .try_send(MessageInner::ReceiverRtcp(self.receiver_id, packets))
            .map_err(|e| Error::Other(format!("{:?}", e)))
    }

    async fn poll(&self) -> Option<TrackRemoteEvent> {
        self.evt_rx.lock().await.recv().await
    }
}
