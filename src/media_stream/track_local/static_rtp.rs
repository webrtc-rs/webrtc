use crate::error::{Error, Result};
use crate::media_stream::Track;
use crate::media_stream::track_local::{TrackLocal, TrackLocalContext};
use crate::peer_connection::driver::PeerConnectionDriverEvent;
use crate::runtime::Mutex;
use bytes::BytesMut;
use rtc::media_stream::{
    MediaStreamId, MediaStreamTrack, MediaStreamTrackId, MediaStreamTrackState,
    MediaTrackCapabilities, MediaTrackConstraints, MediaTrackSettings,
};
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpEncodingParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RtpStreamId, SSRC};
use rtc::shared::error::flatten_errs;
use rtc::shared::marshal::{Marshal, MarshalSize};
use rtc::{rtcp, rtp};
use std::collections::HashMap;

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Clone)]
pub struct TrackLocalStaticRTP {
    pub(crate) track: Mutex<MediaStreamTrack>,
    pub(crate) ctx: Mutex<Option<TrackLocalContext>>,
}

impl TrackLocalStaticRTP {
    pub fn new(track: MediaStreamTrack) -> Self {
        Self {
            track: Mutex::new(track),
            ctx: Mutex::new(None),
        }
    }

    pub async fn write_rtp_with_extensions(
        &self,
        mut pkt: rtp::Packet,
        extensions: &[rtp::extension::HeaderExtension],
    ) -> Result<()> {
        let mut write_errs = vec![];

        // Prepare the extensions data
        let extension_data: HashMap<_, _> = extensions
            .iter()
            .flat_map(|extension| {
                let buf = {
                    let mut buf = BytesMut::with_capacity(extension.marshal_size());
                    buf.resize(extension.marshal_size(), 0);
                    if let Err(err) = extension.marshal_to(&mut buf) {
                        write_errs.push(err);
                        return None;
                    }

                    buf.freeze()
                };

                Some((extension.uri(), buf))
            })
            .collect();

        {
            let ctx = self.ctx.lock().await;
            if let Some(ctx) = &*ctx {
                for (uri, data) in extension_data.iter() {
                    if let Some(id) = ctx
                        .rtp_parameters
                        .header_extensions
                        .iter()
                        .find(|ext| &ext.uri == uri)
                        .map(|ext| ext.id)
                        && let Err(err) = pkt.header.set_extension(id as u8, data.clone())
                    {
                        write_errs.push(err);
                        continue;
                    }
                }
            } else {
                return Err(Error::ErrBindFailed);
            }
        }

        if let Err(err) = self.write_rtp(pkt).await {
            write_errs.push(err);
        }

        flatten_errs(write_errs)
    }
}

#[async_trait::async_trait]
impl Track for TrackLocalStaticRTP {
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
impl TrackLocal for TrackLocalStaticRTP {
    async fn track(&self) -> MediaStreamTrack {
        let track = self.track.lock().await;
        track.clone()
    }

    async fn bind(&self, ctx: TrackLocalContext) {
        let mut ctx_opt = self.ctx.lock().await;
        *ctx_opt = Some(ctx);
    }

    async fn unbind(&self) {
        let mut ctx_opt = self.ctx.lock().await;
        *ctx_opt = None;
    }

    async fn write_rtp(&self, packet: rtp::Packet) -> Result<()> {
        let ctx_opt = self.ctx.lock().await;
        if let Some(ctx) = &*ctx_opt {
            ctx.driver_event_tx
                .try_send(PeerConnectionDriverEvent::SenderRtp(
                    ctx.rtp_sender_id,
                    packet,
                ))
                .map_err(|e| Error::Other(format!("{:?}", e)))
        } else {
            Err(Error::Other("track is not binding yet".to_string()))
        }
    }

    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> Result<()> {
        let ctx_opt = self.ctx.lock().await;
        if let Some(ctx) = &*ctx_opt {
            ctx.driver_event_tx
                .try_send(PeerConnectionDriverEvent::SenderRtcp(
                    ctx.rtp_sender_id,
                    packets,
                ))
                .map_err(|e| Error::Other(format!("{:?}", e)))
        } else {
            Err(Error::Other("track is not binding yet".to_string()))
        }
    }
}
