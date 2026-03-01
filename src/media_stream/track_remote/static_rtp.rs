use crate::error::{Error, Result};
use crate::media_stream::Track;
use crate::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use crate::peer_connection::driver::PeerConnectionDriverEvent;
use crate::runtime::{Mutex, Receiver, Sender};
use rtc::media_stream::{
    MediaStreamId, MediaStreamTrack, MediaStreamTrackId, MediaStreamTrackState,
    MediaTrackCapabilities, MediaTrackConstraints, MediaTrackSettings,
};
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpEncodingParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RTCRtpReceiverId, RtpStreamId, SSRC};

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Clone)]
pub(crate) struct TrackRemoteStaticRTP {
    track: Mutex<MediaStreamTrack>,
    receiver_id: RTCRtpReceiverId,
    msg_tx: Sender<PeerConnectionDriverEvent>,
    evt_rx: Mutex<Receiver<TrackRemoteEvent>>,
}

impl TrackRemoteStaticRTP {
    pub fn new(
        track: MediaStreamTrack,
        receiver_id: RTCRtpReceiverId,
        msg_tx: Sender<PeerConnectionDriverEvent>,
        evt_rx: Receiver<TrackRemoteEvent>,
    ) -> Self {
        Self {
            track: Mutex::new(track),
            receiver_id,
            msg_tx,
            evt_rx: Mutex::new(evt_rx),
        }
    }
}

#[async_trait::async_trait]
impl Track for TrackRemoteStaticRTP {
    async fn stream_id(&self) -> MediaStreamId {
        let track = self.track.lock().await;
        track.stream_id().to_owned()
    }

    async fn track_id(&self) -> MediaStreamTrackId {
        let track = self.track.lock().await;
        track.track_id().to_owned()
    }

    async fn label(&self) -> String {
        let track = self.track.lock().await;
        track.label().to_owned()
    }

    async fn kind(&self) -> RtpCodecKind {
        let track = self.track.lock().await;
        track.kind()
    }

    async fn rid(&self, ssrc: SSRC) -> Option<RtpStreamId> {
        let track = self.track.lock().await;
        track.rid(ssrc).cloned()
    }

    async fn codec(&self, ssrc: SSRC) -> Option<RTCRtpCodec> {
        let track = self.track.lock().await;
        track.codec(ssrc).cloned()
    }

    async fn ssrcs(&self) -> Vec<SSRC> {
        let track = self.track.lock().await;
        track.ssrcs().collect()
    }

    async fn enabled(&self) -> bool {
        let track = self.track.lock().await;
        track.enabled()
    }

    async fn set_enabled(&self, enabled: bool) {
        let mut track = self.track.lock().await;
        track.set_enabled(enabled);
    }

    async fn muted(&self) -> bool {
        let track = self.track.lock().await;
        track.muted()
    }

    async fn set_muted(&self, muted: bool) {
        let mut track = self.track.lock().await;
        track.set_muted(muted);
    }

    async fn ready_state(&self) -> MediaStreamTrackState {
        let track = self.track.lock().await;
        track.ready_state()
    }

    async fn stop(&self) {
        let mut track = self.track.lock().await;
        track.stop();
    }

    async fn get_capabilities(&self) -> MediaTrackCapabilities {
        let track = self.track.lock().await;
        track.get_capabilities().clone()
    }

    async fn get_constraints(&self) -> MediaTrackConstraints {
        let track = self.track.lock().await;
        track.get_constraints().clone()
    }

    async fn get_settings(&self) -> MediaTrackSettings {
        let track = self.track.lock().await;
        track.get_settings().clone()
    }

    async fn apply_constraints(&self, constraints: Option<MediaTrackConstraints>) {
        let mut track = self.track.lock().await;
        track.apply_constraints(constraints);
    }

    async fn codings(&self) -> Vec<RTCRtpEncodingParameters> {
        let track = self.track.lock().await;
        track.codings().to_vec()
    }

    async fn add_coding(&self, coding: RTCRtpEncodingParameters) {
        let mut track = self.track.lock().await;
        track.add_coding(coding);
    }
}

#[async_trait::async_trait]
impl TrackRemote for TrackRemoteStaticRTP {
    async fn write_rtcp(&self, packets: Vec<Box<dyn rtc::rtcp::Packet>>) -> Result<()> {
        self.msg_tx
            .try_send(PeerConnectionDriverEvent::ReceiverRtcp(
                self.receiver_id,
                packets,
            ))
            .map_err(|e| Error::Other(format!("{:?}", e)))
    }

    async fn poll(&self) -> Option<TrackRemoteEvent> {
        self.evt_rx.lock().await.recv().await
    }
}
