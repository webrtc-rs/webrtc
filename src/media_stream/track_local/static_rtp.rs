use crate::error::{Error, Result};
use crate::media_stream::Track;
use crate::media_stream::track_local::{TrackLocal, TrackLocalContext};
use crate::peer_connection::driver::PeerConnectionDriverEvent;
use crate::runtime::Mutex;
use rtc::media_stream::MediaStreamTrack;
use rtc::{rtcp, rtp};

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Clone)]
pub struct TrackLocalStaticRTP {
    track: MediaStreamTrack,
    ctx: Mutex<Option<TrackLocalContext>>,
}

impl TrackLocalStaticRTP {
    pub fn new(track: MediaStreamTrack) -> Self {
        Self {
            track,
            ctx: Mutex::new(None),
        }
    }
}

impl Track for TrackLocalStaticRTP {
    fn track(&self) -> &MediaStreamTrack {
        &self.track
    }
}

#[async_trait::async_trait]
impl TrackLocal for TrackLocalStaticRTP {
    async fn bind(&self, ctx: TrackLocalContext) {
        let mut ctx_opt = self.ctx.lock().await;
        *ctx_opt = Some(ctx);
    }

    async fn unbind(&self) {
        let mut ctx_opt = self.ctx.lock().await;
        *ctx_opt = None;
    }

    async fn write_rtp(&self, mut packet: rtp::Packet) -> Result<()> {
        //TODO: make it more comprehensive handling
        packet.header.ssrc = self
            .track
            .ssrcs()
            .next()
            .ok_or(Error::ErrSenderWithNoSSRCs)?;

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
