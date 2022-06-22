use crate::api::media_engine::MediaEngine;
use crate::error::{Error, Result};
use crate::rtp_transceiver::rtp_codec::{RTCRtpCodecParameters, RTCRtpParameters, RTPCodecType};
use crate::rtp_transceiver::{PayloadType, SSRC};
use crate::RECEIVE_MTU;

use crate::rtp_transceiver::rtp_receiver::RTPReceiverInternal;

use crate::track::RTP_PAYLOAD_TYPE_BITMASK;
use bytes::{Bytes, BytesMut};
use interceptor::{Attributes, Interceptor};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;
use util::Unmarshal;

lazy_static! {
    static ref TRACK_REMOTE_UNIQUE_ID: AtomicUsize = AtomicUsize::new(0);
}
pub type OnMuteHdlrFn = Box<
    dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync + 'static,
>;

#[derive(Default)]
struct Handlers {
    on_mute: Option<OnMuteHdlrFn>,
    on_unmute: Option<OnMuteHdlrFn>,
}

#[derive(Default)]
struct TrackRemoteInternal {
    peeked: Option<Bytes>,
    peeked_attributes: Option<Attributes>,
}

/// TrackRemote represents a single inbound source of media
pub struct TrackRemote {
    tid: usize,

    id: Mutex<String>,
    stream_id: Mutex<String>,

    receive_mtu: usize,
    payload_type: AtomicU8, //PayloadType,
    kind: AtomicU8,         //RTPCodecType,
    ssrc: AtomicU32,        //SSRC,
    codec: Mutex<RTCRtpCodecParameters>,
    pub(crate) params: Mutex<RTCRtpParameters>,
    rid: String,

    media_engine: Arc<MediaEngine>,
    interceptor: Arc<dyn Interceptor + Send + Sync>,

    handlers: Mutex<Handlers>,

    receiver: Option<Weak<RTPReceiverInternal>>,
    internal: Mutex<TrackRemoteInternal>,
}

impl std::fmt::Debug for TrackRemote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackRemote")
            .field("id", &self.id)
            .field("stream_id", &self.stream_id)
            .field("payload_type", &self.payload_type)
            .field("kind", &self.kind)
            .field("ssrc", &self.ssrc)
            .field("codec", &self.codec)
            .field("params", &self.params)
            .field("rid", &self.rid)
            .finish()
    }
}

impl TrackRemote {
    pub(crate) fn new(
        receive_mtu: usize,
        kind: RTPCodecType,
        ssrc: SSRC,
        rid: String,
        receiver: Weak<RTPReceiverInternal>,
        media_engine: Arc<MediaEngine>,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
    ) -> Self {
        TrackRemote {
            tid: TRACK_REMOTE_UNIQUE_ID.fetch_add(1, Ordering::SeqCst),
            id: Default::default(),
            stream_id: Default::default(),
            receive_mtu,
            payload_type: Default::default(),
            kind: AtomicU8::new(kind as u8),
            ssrc: AtomicU32::new(ssrc),
            codec: Default::default(),
            params: Default::default(),
            rid,
            receiver: Some(receiver),
            media_engine,
            interceptor,
            handlers: Default::default(),

            internal: Default::default(),
        }
    }

    pub fn tid(&self) -> usize {
        self.tid
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
    pub async fn codec(&self) -> RTCRtpCodecParameters {
        let codec = self.codec.lock().await;
        codec.clone()
    }

    pub async fn set_codec(&self, codec: RTCRtpCodecParameters) {
        let mut c = self.codec.lock().await;
        *c = codec;
    }

    pub async fn params(&self) -> RTCRtpParameters {
        let p = self.params.lock().await;
        p.clone()
    }

    pub async fn set_params(&self, params: RTCRtpParameters) {
        let mut p = self.params.lock().await;
        *p = params;
    }

    pub async fn onmute<F>(&self, handler: F)
    where
        F: FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> + Send + 'static + Sync,
    {
        let mut handlers = self.handlers.lock().await;
        handlers.on_mute = Some(Box::new(handler));
    }

    pub async fn onunmute<F>(&self, handler: F)
    where
        F: FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> + Send + 'static + Sync,
    {
        let mut handlers = self.handlers.lock().await;
        handlers.on_unmute = Some(Box::new(handler));
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
                    if let Some(receiver) = receiver.upgrade() {
                        receiver.read_rtp(b, self.tid).await?
                    } else {
                        return Err(Error::ErrRTPReceiverNil);
                    }
                } else {
                    return Err(Error::ErrRTPReceiverNil);
                }
            };
            self.check_and_update_track(&b[..n]).await?;
            Ok((n, attributes))
        }
    }

    /// check_and_update_track checks payloadType for every incoming packet
    /// once a different payloadType is detected the track will be updated
    pub(crate) async fn check_and_update_track(&self, b: &[u8]) -> Result<()> {
        if b.len() < 2 {
            return Err(Error::ErrRTPTooShort);
        }

        let payload_type = b[1] & RTP_PAYLOAD_TYPE_BITMASK;
        if payload_type != self.payload_type() {
            let p = self
                .media_engine
                .get_rtp_parameters_by_payload_type(payload_type)
                .await?;

            if let Some(receiver) = &self.receiver {
                if let Some(receiver) = receiver.upgrade() {
                    self.kind.store(receiver.kind as u8, Ordering::SeqCst);
                }
            }
            self.payload_type.store(payload_type, Ordering::SeqCst);
            {
                let mut codec = self.codec.lock().await;
                *codec = if let Some(codec) = p.codecs.first() {
                    codec.clone()
                } else {
                    return Err(Error::ErrCodecNotFound);
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
        let mut b = vec![0u8; self.receive_mtu];
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
    pub(crate) async fn peek(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
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

    pub(crate) async fn fire_onmute(&self) {
        let mut handlers = self.handlers.lock().await;

        match &mut handlers.on_mute {
            Some(f) => f().await,
            None => {}
        };
    }

    pub(crate) async fn fire_onunmute(&self) {
        let mut handlers = self.handlers.lock().await;

        match &mut handlers.on_unmute {
            Some(f) => f().await,
            None => {}
        };
    }
}
