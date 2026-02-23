use crate::error::{Error, Result};
use crate::media_stream::Track;
use crate::media_stream::track_local::{TrackLocal, TrackLocalContext};
use crate::peer_connection::driver::PeerConnectionDriverEvent;
use crate::runtime::Mutex;
use bytes::BytesMut;
use rtc::media_stream::MediaStreamTrack;
use rtc::shared::error::flatten_errs;
use rtc::shared::marshal::{Marshal, MarshalSize};
use rtc::{rtcp, rtp};
use std::collections::HashMap;

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Clone)]
pub struct TrackLocalStaticRTP {
    pub(crate) track: MediaStreamTrack,
    pub(crate) ctx: Mutex<Option<TrackLocalContext>>,
}

impl TrackLocalStaticRTP {
    pub fn new(track: MediaStreamTrack) -> Self {
        Self {
            track,
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
