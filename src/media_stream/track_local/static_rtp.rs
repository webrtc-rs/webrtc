use crate::error::{Error, Result};
use crate::media_stream::Track;
use crate::media_stream::track_local::{TrackLocal, TrackLocalContext};
use crate::peer_connection::MessageInner;
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
    async fn bind(&self, ctx: &TrackLocalContext) -> Result<()> {
        let mut ctx_opt = self.ctx.lock().await;
        *ctx_opt = Some(ctx.clone());
        Ok(())
    }

    async fn unbind(&self) -> Result<()> {
        let mut ctx_opt = self.ctx.lock().await;
        *ctx_opt = None;
        Ok(())
    }

    async fn write_rtp(&self, packet: rtp::Packet) -> Result<()> {
        let ctx_opt = self.ctx.lock().await;
        if let Some(ctx) = &*ctx_opt {
            ctx.msg_tx
                .try_send(MessageInner::SenderRtp(ctx.sender_id, packet))
                .map_err(|e| Error::Other(format!("{:?}", e)))
        } else {
            Err(Error::Other("track is not binding yet".to_string()))
        }
    }

    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> Result<()> {
        let ctx_opt = self.ctx.lock().await;
        if let Some(ctx) = &*ctx_opt {
            ctx.msg_tx
                .try_send(MessageInner::SenderRtcp(ctx.sender_id, packets))
                .map_err(|e| Error::Other(format!("{:?}", e)))
        } else {
            Err(Error::Other("track is not binding yet".to_string()))
        }
    }
}
