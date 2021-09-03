use crate::api::media_engine::MediaEngine;
use crate::error::Error;
use crate::media::interceptor::{Attributes, Interceptor};
use crate::media::rtp::rtp_codec::{RTPCodecParameters, RTPCodecType, RTPParameters};
use crate::media::rtp::{PayloadType, SSRC};
use crate::{RECEIVE_MTU, RTP_PAYLOAD_TYPE_BITMASK};

use crate::media::rtp::rtp_receiver::RTPReceiverInternal;
use anyhow::Result;
use bytes::{Bytes, BytesMut};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use util::Unmarshal;

#[derive(Default)]
struct TrackRemoteInternal {
    peeked: Option<Bytes>,
    peeked_attributes: Option<Attributes>,
}

/// TrackRemote represents a single inbound source of media
#[derive(Default)]
pub struct TrackRemote {
    id: Mutex<String>,
    stream_id: Mutex<String>,

    payload_type: AtomicU8, //PayloadType,
    kind: AtomicU8,         //RTPCodecType,
    ssrc: AtomicU32,        //SSRC,
    codec: Mutex<RTPCodecParameters>,
    pub(crate) params: Mutex<RTPParameters>,
    rid: String,

    media_engine: Arc<MediaEngine>,
    interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,

    receiver: Option<Arc<Mutex<RTPReceiverInternal>>>,
    internal: Mutex<TrackRemoteInternal>,
}

impl TrackRemote {
    pub(crate) fn new(
        kind: RTPCodecType,
        ssrc: SSRC,
        rid: String,
        receiver: Arc<Mutex<RTPReceiverInternal>>,
        media_engine: Arc<MediaEngine>,
        interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,
    ) -> Self {
        TrackRemote {
            kind: AtomicU8::new(kind as u8),
            ssrc: AtomicU32::new(ssrc),
            rid,
            receiver: Some(receiver),
            media_engine,
            interceptor,
            ..Default::default()
        }
    }

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and StreamID would be 'desktop' or 'webcam'
    pub async fn id(&self) -> String {
        let id = self.id.lock().await;
        id.clone()
    }

    pub async fn set_id(&self, s: String) {
        let mut id = self.id.lock().await;
        *id = s;
    }

    /// stream_id is the group this track belongs too. This must be unique
    pub async fn stream_id(&self) -> String {
        let stream_id = self.stream_id.lock().await;
        stream_id.clone()
    }

    pub async fn set_stream_id(&self, s: String) {
        let mut stream_id = self.stream_id.lock().await;
        *stream_id = s;
    }

    /// rid gets the RTP Stream ID of this Track
    /// With Simulcast you will have multiple tracks with the same ID, but different RID values.
    /// In many cases a TrackRemote will not have an RID, so it is important to assert it is non-zero
    pub fn rid(&self) -> &str {
        self.rid.as_str()
    }

    /// payload_type gets the PayloadType of the track
    pub fn payload_type(&self) -> PayloadType {
        self.payload_type.load(Ordering::SeqCst)
    }

    pub fn set_payload_type(&self, payload_type: PayloadType) {
        self.payload_type.store(payload_type, Ordering::SeqCst);
    }

    /// kind gets the Kind of the track
    pub fn kind(&self) -> RTPCodecType {
        self.kind.load(Ordering::SeqCst).into()
    }

    pub fn set_kind(&self, kind: RTPCodecType) {
        self.kind.store(kind as u8, Ordering::SeqCst);
    }

    /// ssrc gets the SSRC of the track
    pub fn ssrc(&self) -> SSRC {
        self.ssrc.load(Ordering::SeqCst)
    }

    pub fn set_ssrc(&self, ssrc: SSRC) {
        self.ssrc.store(ssrc, Ordering::SeqCst);
    }

    /// msid gets the Msid of the track
    pub async fn msid(&self) -> String {
        self.stream_id().await + " " + self.id().await.as_str()
    }

    /// codec gets the Codec of the track
    pub async fn codec(&self) -> RTPCodecParameters {
        let codec = self.codec.lock().await;
        codec.clone()
    }

    pub async fn set_codec(&self, codec: RTPCodecParameters) {
        let mut c = self.codec.lock().await;
        *c = codec;
    }

    pub async fn params(&self) -> RTPParameters {
        let p = self.params.lock().await;
        p.clone()
    }

    pub async fn set_params(&self, params: RTPParameters) {
        let mut p = self.params.lock().await;
        *p = params;
    }

    /// Read reads data from the track.
    pub async fn read(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        let (peeked, peeked_attributes) = {
            let mut internal = self.internal.lock().await;
            (internal.peeked.take(), internal.peeked_attributes.take())
        };

        if let (Some(data), Some(attributes)) = (peeked, peeked_attributes) {
            // someone else may have stolen our packet when we
            // released the lock.  Deal with it.
            let n = std::cmp::min(b.len(), data.len());
            b[..n].copy_from_slice(&data[..n]);
            self.check_and_update_track(&b[..n]).await?;
            Ok((n, attributes))
        } else {
            let (n, attributes) = {
                if let Some(receiver) = &self.receiver {
                    let mut internal = receiver.lock().await;
                    internal.read_rtp(b, self.id().await.as_str()).await?
                } else {
                    return Err(Error::ErrRTPReceiverNil.into());
                }
            };
            self.check_and_update_track(&b[..n]).await?;
            Ok((n, attributes))
        }
    }

    /// check_and_update_track checks payloadType for every incoming packet
    /// once a different payloadType is detected the track will be updated
    async fn check_and_update_track(&self, b: &[u8]) -> Result<()> {
        if b.len() < 2 {
            return Err(Error::ErrRTPTooShort.into());
        }

        let payload_type = b[1] & RTP_PAYLOAD_TYPE_BITMASK;
        if payload_type != self.payload_type() {
            let p = self
                .media_engine
                .get_rtp_parameters_by_payload_type(payload_type)
                .await?;

            if let Some(receiver) = &self.receiver {
                let r = receiver.lock().await;
                self.kind.store(r.kind as u8, Ordering::SeqCst);
            }
            self.payload_type.store(payload_type, Ordering::SeqCst);
            {
                let mut codec = self.codec.lock().await;
                *codec = if let Some(codec) = p.codecs.first() {
                    codec.clone()
                } else {
                    return Err(Error::ErrCodecNotFound.into());
                };
            }
            {
                let mut params = self.params.lock().await;
                *params = p;
            }
        }

        Ok(())
    }

    /// read_rtp is a convenience method that wraps Read and unmarshals for you.
    pub async fn read_rtp(&self) -> Result<(rtp::packet::Packet, Attributes)> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, attributes) = self.read(&mut b).await?;

        let mut buf = &b[..n];
        let r = rtp::packet::Packet::unmarshal(&mut buf)?;
        Ok((r, attributes))
    }

    /// determine_payload_type blocks and reads a single packet to determine the PayloadType for this Track
    /// this is useful because we can't announce it to the user until we know the payload_type
    pub(crate) async fn determine_payload_type(&self) -> Result<()> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, _) = self.peek(&mut b).await?;

        let mut buf = &b[..n];
        let r = rtp::packet::Packet::unmarshal(&mut buf)?;
        self.payload_type
            .store(r.header.payload_type, Ordering::SeqCst);

        Ok(())
    }

    /// peek is like Read, but it doesn't discard the packet read
    async fn peek(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        let (n, a) = self.read(b).await?;

        // this might overwrite data if somebody peeked between the Read
        // and us getting the lock.  Oh well, we'll just drop a packet in
        // that case.
        let mut data = BytesMut::new();
        data.extend(b[..n].to_vec());
        {
            let mut internal = self.internal.lock().await;
            internal.peeked = Some(data.freeze());
            internal.peeked_attributes = Some(a.clone());
        }
        Ok((n, a))
    }
}
