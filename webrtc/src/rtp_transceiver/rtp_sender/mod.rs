#[cfg(test)]
mod rtp_sender_test;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use ice::rand::generate_crypto_random_string;
use interceptor::stream_info::{AssociatedStreamInfo, StreamInfo};
use interceptor::{Attributes, Interceptor, RTCPReader, RTPWriter};
use portable_atomic::AtomicBool;
use tokio::select;
use tokio::sync::{watch, Mutex, Notify};
use util::sync::Mutex as SyncMutex;

use super::srtp_writer_future::SequenceTransformer;
use super::RTCRtpRtxParameters;
use crate::api::media_engine::MediaEngine;
use crate::api::setting_engine::SettingEngine;
use crate::dtls_transport::RTCDtlsTransport;
use crate::error::{Error, Result};
use crate::rtp_transceiver::rtp_codec::{codec_rtx_search, RTPCodecType};
use crate::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use crate::rtp_transceiver::srtp_writer_future::SrtpWriterFuture;
use crate::rtp_transceiver::{
    create_stream_info, PayloadType, RTCRtpEncodingParameters, RTCRtpSendParameters,
    RTCRtpTransceiver, SSRC,
};
use crate::track::track_local::{
    InterceptorToTrackLocalWriter, TrackLocal, TrackLocalContext, TrackLocalWriter,
};

pub(crate) struct RTPSenderInternal {
    pub(crate) stop_called_rx: Arc<Notify>,
    pub(crate) stop_called_signal: Arc<AtomicBool>,
}

pub(crate) struct TrackEncoding {
    pub(crate) track: Arc<dyn TrackLocal + Send + Sync>,
    pub(crate) srtp_stream: Arc<SrtpWriterFuture>,
    pub(crate) rtcp_interceptor: Arc<dyn RTCPReader + Send + Sync>,
    pub(crate) stream_info: Mutex<StreamInfo>,
    pub(crate) context: Mutex<TrackLocalContext>,

    pub(crate) ssrc: SSRC,

    pub(crate) rtx: Option<RtxEncoding>,
}

pub(crate) struct RtxEncoding {
    pub(crate) srtp_stream: Arc<SrtpWriterFuture>,
    pub(crate) rtcp_interceptor: Arc<dyn RTCPReader + Send + Sync>,
    pub(crate) stream_info: Mutex<StreamInfo>,

    pub(crate) ssrc: SSRC,
}

/// RTPSender allows an application to control how a given Track is encoded and transmitted to a remote peer
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCRtpSender
/// [W3C]: https://w3c.github.io/webrtc-pc/#rtcrtpsender-interface
pub struct RTCRtpSender {
    pub(crate) track_encodings: Mutex<Vec<TrackEncoding>>,

    seq_trans: Arc<SequenceTransformer>,
    rtx_seq_trans: Arc<SequenceTransformer>,

    pub(crate) transport: Arc<RTCDtlsTransport>,

    pub(crate) kind: RTPCodecType,
    pub(crate) payload_type: PayloadType,
    receive_mtu: usize,
    enable_rtx: bool,

    /// a transceiver sender since we can just check the
    /// transceiver negotiation status
    pub(crate) negotiated: AtomicBool,

    pub(crate) media_engine: Arc<MediaEngine>,
    pub(crate) interceptor: Arc<dyn Interceptor + Send + Sync>,

    pub(crate) id: String,

    /// The id of the initial track, even if we later change to a different
    /// track id should be use when negotiating.
    pub(crate) initial_track_id: std::sync::Mutex<Option<String>>,
    /// AssociatedMediaStreamIds from the WebRTC specifications
    pub(crate) associated_media_stream_ids: std::sync::Mutex<Vec<String>>,

    rtp_transceiver: SyncMutex<Option<Weak<RTCRtpTransceiver>>>,

    send_called: watch::Sender<bool>,
    stop_called_tx: Arc<Notify>,
    stop_called_signal: Arc<AtomicBool>,

    pub(crate) paused: Arc<AtomicBool>,

    internal: Arc<RTPSenderInternal>,
}

impl std::fmt::Debug for RTCRtpSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RTCRtpSender")
            .field("id", &self.id)
            .finish()
    }
}

impl RTCRtpSender {
    pub async fn new(
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
        kind: RTPCodecType,
        transport: Arc<RTCDtlsTransport>,
        media_engine: Arc<MediaEngine>,
        setting_engine: Arc<SettingEngine>,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
        start_paused: bool,
    ) -> Self {
        let id = generate_crypto_random_string(
            32,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
        );
        let (send_called, _) = watch::channel(false);
        let stop_called_tx = Arc::new(Notify::new());
        let stop_called_rx = stop_called_tx.clone();
        let stop_called_signal = Arc::new(AtomicBool::new(false));

        let internal = Arc::new(RTPSenderInternal {
            stop_called_rx,
            stop_called_signal: Arc::clone(&stop_called_signal),
        });

        let seq_trans = Arc::new(SequenceTransformer::new());
        let rtx_seq_trans = Arc::new(SequenceTransformer::new());

        let stream_ids = track
            .as_ref()
            .map(|track| vec![track.stream_id().to_string()])
            .unwrap_or_default();
        let ret = Self {
            track_encodings: Mutex::new(vec![]),

            seq_trans,
            rtx_seq_trans,

            transport,

            kind,
            payload_type: 0,
            receive_mtu: setting_engine.get_receive_mtu(),
            enable_rtx: setting_engine.enable_sender_rtx,

            negotiated: AtomicBool::new(false),

            media_engine,
            interceptor,

            id,
            initial_track_id: std::sync::Mutex::new(None),
            associated_media_stream_ids: std::sync::Mutex::new(stream_ids),

            rtp_transceiver: SyncMutex::new(None),

            send_called,
            stop_called_tx,
            stop_called_signal,

            paused: Arc::new(AtomicBool::new(start_paused)),

            internal,
        };

        if let Some(track) = track {
            let mut track_encodings = ret.track_encodings.lock().await;
            let _ = ret.add_encoding_internal(&mut track_encodings, track).await;
        }

        ret
    }

    /// AddEncoding adds an encoding to RTPSender. Used by simulcast senders.
    pub async fn add_encoding(&self, track: Arc<dyn TrackLocal + Send + Sync>) -> Result<()> {
        let mut track_encodings = self.track_encodings.lock().await;

        if track.rid().is_none() {
            return Err(Error::ErrRTPSenderRidNil);
        }

        if self.has_stopped().await {
            return Err(Error::ErrRTPSenderStopped);
        }

        if self.has_sent() {
            return Err(Error::ErrRTPSenderSendAlreadyCalled);
        }

        let base_track = track_encodings
            .first()
            .map(|e| &e.track)
            .ok_or(Error::ErrRTPSenderNoBaseEncoding)?;
        if base_track.rid().is_none() {
            return Err(Error::ErrRTPSenderNoBaseEncoding);
        }

        if base_track.id() != track.id()
            || base_track.stream_id() != track.stream_id()
            || base_track.kind() != track.kind()
        {
            return Err(Error::ErrRTPSenderBaseEncodingMismatch);
        }

        if track_encodings.iter().any(|e| e.track.rid() == track.rid()) {
            return Err(Error::ErrRTPSenderRIDCollision);
        }

        self.add_encoding_internal(&mut track_encodings, track)
            .await
    }

    async fn add_encoding_internal(
        &self,
        track_encodings: &mut Vec<TrackEncoding>,
        track: Arc<dyn TrackLocal + Send + Sync>,
    ) -> Result<()> {
        let ssrc = rand::random::<u32>();
        let srtp_stream = Arc::new(SrtpWriterFuture {
            closed: AtomicBool::new(false),
            ssrc,
            rtp_sender: Arc::downgrade(&self.internal),
            rtp_transport: Arc::clone(&self.transport),
            rtcp_read_stream: Mutex::new(None),
            rtp_write_session: Mutex::new(None),
            seq_trans: Arc::clone(&self.seq_trans),
        });

        let srtp_rtcp_reader = Arc::clone(&srtp_stream) as Arc<dyn RTCPReader + Send + Sync>;
        let rtcp_interceptor = self.interceptor.bind_rtcp_reader(srtp_rtcp_reader).await;

        let create_rtx_stream = self.enable_rtx
            && self
                .media_engine
                .get_codecs_by_kind(track.kind())
                .iter()
                .any(|codec| {
                    matches!(codec.capability.mime_type.split_once("/"), Some((_, "rtx")))
                });

        let rtx = if create_rtx_stream {
            let ssrc = rand::random::<u32>();

            let srtp_stream = Arc::new(SrtpWriterFuture {
                closed: AtomicBool::new(false),
                ssrc,
                rtp_sender: Arc::downgrade(&self.internal),
                rtp_transport: Arc::clone(&self.transport),
                rtcp_read_stream: Mutex::new(None),
                rtp_write_session: Mutex::new(None),
                seq_trans: Arc::clone(&self.rtx_seq_trans),
            });

            let srtp_rtcp_reader = Arc::clone(&srtp_stream) as Arc<dyn RTCPReader + Send + Sync>;
            let rtcp_interceptor = self.interceptor.bind_rtcp_reader(srtp_rtcp_reader).await;

            Some(RtxEncoding {
                srtp_stream,
                rtcp_interceptor,
                stream_info: Mutex::new(StreamInfo::default()),
                ssrc,
            })
        } else {
            None
        };

        let encoding = TrackEncoding {
            track,
            srtp_stream,
            rtcp_interceptor,
            stream_info: Mutex::new(StreamInfo::default()),
            context: Mutex::new(TrackLocalContext::default()),
            ssrc,
            rtx,
        };

        track_encodings.push(encoding);

        Ok(())
    }

    pub(crate) fn is_negotiated(&self) -> bool {
        self.negotiated.load(Ordering::SeqCst)
    }

    pub(crate) fn set_negotiated(&self) {
        self.negotiated.store(true, Ordering::SeqCst);
    }

    pub(crate) fn set_rtp_transceiver(&self, rtp_transceiver: Option<Weak<RTCRtpTransceiver>>) {
        if let Some(t) = rtp_transceiver.as_ref().and_then(|t| t.upgrade()) {
            self.set_paused(!t.direction().has_send());
        }
        let mut tr = self.rtp_transceiver.lock();
        *tr = rtp_transceiver;
    }

    pub(crate) fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    /// transport returns the currently-configured DTLSTransport
    /// if one has not yet been configured
    pub fn transport(&self) -> Arc<RTCDtlsTransport> {
        Arc::clone(&self.transport)
    }

    /// get_parameters describes the current configuration for the encoding and
    /// transmission of media on the sender's track.
    pub async fn get_parameters(&self) -> RTCRtpSendParameters {
        let encodings = {
            let track_encodings = self.track_encodings.lock().await;
            let mut encodings = Vec::with_capacity(track_encodings.len());
            for e in track_encodings.iter() {
                encodings.push(RTCRtpEncodingParameters {
                    rid: e.track.rid().unwrap_or_default().into(),
                    ssrc: e.ssrc,
                    payload_type: self.payload_type,
                    rtx: RTCRtpRtxParameters {
                        ssrc: e.rtx.as_ref().map(|e| e.ssrc).unwrap_or_default(),
                    },
                });
            }

            encodings
        };

        let mut rtp_parameters = self
            .media_engine
            .get_rtp_parameters_by_kind(self.kind, RTCRtpTransceiverDirection::Sendonly);
        rtp_parameters.codecs = {
            let tr = self
                .rtp_transceiver
                .lock()
                .clone()
                .and_then(|t| t.upgrade());
            if let Some(t) = &tr {
                t.get_codecs().await
            } else {
                self.media_engine.get_codecs_by_kind(self.kind)
            }
        };

        RTCRtpSendParameters {
            rtp_parameters,
            encodings,
        }
    }

    /// track returns the RTCRtpTransceiver track, or nil
    pub async fn track(&self) -> Option<Arc<dyn TrackLocal + Send + Sync>> {
        self.track_encodings
            .lock()
            .await
            .first()
            .map(|e| Arc::clone(&e.track))
    }

    /// replace_track replaces the track currently being used as the sender's source with a new TrackLocal.
    /// The new track must be of the same media kind (audio, video, etc) and switching the track should not
    /// require negotiation.
    pub async fn replace_track(
        &self,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        let mut track_encodings = self.track_encodings.lock().await;

        if let Some(t) = &track {
            if self.kind != t.kind() {
                return Err(Error::ErrRTPSenderNewTrackHasIncorrectKind);
            }

            // cannot replace simulcast envelope
            if track_encodings.len() > 1 {
                return Err(Error::ErrRTPSenderNewTrackHasIncorrectEnvelope);
            }

            let encoding = track_encodings
                .first_mut()
                .ok_or(Error::ErrRTPSenderNewTrackHasIncorrectEnvelope)?;

            let mut context = encoding.context.lock().await;
            if self.has_sent() {
                encoding.track.unbind(&context).await?;
            }

            self.seq_trans.reset_offset();
            self.rtx_seq_trans.reset_offset();

            let mid = self
                .rtp_transceiver
                .lock()
                .clone()
                .and_then(|t| t.upgrade())
                .and_then(|t| t.mid());

            let new_context = TrackLocalContext {
                id: context.id.clone(),
                params: self
                    .media_engine
                    .get_rtp_parameters_by_kind(t.kind(), RTCRtpTransceiverDirection::Sendonly),
                ssrc: context.ssrc,
                write_stream: context.write_stream.clone(),
                paused: self.paused.clone(),
                mid,
            };

            match t.bind(&new_context).await {
                Err(err) => {
                    // Re-bind the original track
                    encoding.track.bind(&context).await?;

                    Err(err)
                }
                Ok(codec) => {
                    // Codec has changed
                    context.params.codecs = vec![codec];
                    encoding.track = Arc::clone(t);
                    Ok(())
                }
            }
        } else {
            if self.has_sent() {
                for encoding in track_encodings.drain(..) {
                    let context = encoding.context.lock().await;
                    encoding.track.unbind(&context).await?;
                }
            } else {
                track_encodings.clear();
            }

            Ok(())
        }
    }

    /// send Attempts to set the parameters controlling the sending of media.
    pub async fn send(&self, parameters: &RTCRtpSendParameters) -> Result<()> {
        if self.has_sent() {
            return Err(Error::ErrRTPSenderSendAlreadyCalled);
        }
        let track_encodings = self.track_encodings.lock().await;
        if track_encodings.is_empty() {
            return Err(Error::ErrRTPSenderTrackRemoved);
        }

        let mid = self
            .rtp_transceiver
            .lock()
            .clone()
            .and_then(|t| t.upgrade())
            .and_then(|t| t.mid());

        for (idx, encoding) in track_encodings.iter().enumerate() {
            let write_stream = Arc::new(InterceptorToTrackLocalWriter::new(self.paused.clone()));
            let mut context = TrackLocalContext {
                id: self.id.clone(),
                params: self.media_engine.get_rtp_parameters_by_kind(
                    encoding.track.kind(),
                    RTCRtpTransceiverDirection::Sendonly,
                ),
                ssrc: parameters.encodings[idx].ssrc,
                write_stream: Some(
                    Arc::clone(&write_stream) as Arc<dyn TrackLocalWriter + Send + Sync>
                ),
                paused: self.paused.clone(),
                mid: mid.to_owned(),
            };

            let codec = encoding.track.bind(&context).await?;
            let stream_info = create_stream_info(
                self.id.clone(),
                parameters.encodings[idx].ssrc,
                codec.payload_type,
                codec.capability.clone(),
                &parameters.rtp_parameters.header_extensions,
                None,
            );
            context.params.codecs = vec![codec.clone()];

            let srtp_writer = Arc::clone(&encoding.srtp_stream) as Arc<dyn RTPWriter + Send + Sync>;
            let rtp_writer = self
                .interceptor
                .bind_local_stream(&stream_info, srtp_writer)
                .await;

            *encoding.context.lock().await = context;
            *encoding.stream_info.lock().await = stream_info;
            *write_stream.interceptor_rtp_writer.lock().await = Some(rtp_writer);

            if let (Some(rtx), Some(rtx_codec)) = (
                &encoding.rtx,
                codec_rtx_search(&codec, &parameters.rtp_parameters.codecs),
            ) {
                let rtx_info = AssociatedStreamInfo {
                    ssrc: parameters.encodings[idx].ssrc,
                    payload_type: codec.payload_type,
                };

                let rtx_stream_info = create_stream_info(
                    self.id.clone(),
                    parameters.encodings[idx].rtx.ssrc,
                    rtx_codec.payload_type,
                    rtx_codec.capability.clone(),
                    &parameters.rtp_parameters.header_extensions,
                    Some(rtx_info),
                );

                let rtx_srtp_writer =
                    Arc::clone(&rtx.srtp_stream) as Arc<dyn RTPWriter + Send + Sync>;
                // ignore the rtp writer, only interceptors can write to the stream
                self.interceptor
                    .bind_local_stream(&rtx_stream_info, rtx_srtp_writer)
                    .await;

                *rtx.stream_info.lock().await = rtx_stream_info;

                self.receive_rtcp_for_rtx(rtx.rtcp_interceptor.clone());
            }
        }

        self.send_called.send_replace(true);
        Ok(())
    }

    /// starts a routine that reads the rtx rtcp stream
    /// These packets aren't exposed to the user, but we need to process them
    /// for TWCC
    fn receive_rtcp_for_rtx(&self, rtcp_reader: Arc<dyn RTCPReader + Send + Sync>) {
        let receive_mtu = self.receive_mtu;
        let stop_called_signal = self.internal.stop_called_signal.clone();
        let stop_called_rx = self.internal.stop_called_rx.clone();

        tokio::spawn(async move {
            let attrs = Attributes::new();
            let mut b = vec![0u8; receive_mtu];
            while !stop_called_signal.load(Ordering::SeqCst) {
                select! {
                    r = rtcp_reader.read(&mut b, &attrs) => {
                        if r.is_err() {
                            break
                        }
                    },
                    _ = stop_called_rx.notified() => break,
                }
            }
        });
    }

    /// stop irreversibly stops the RTPSender
    pub async fn stop(&self) -> Result<()> {
        if self.stop_called_signal.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.stop_called_signal.store(true, Ordering::SeqCst);
        self.stop_called_tx.notify_waiters();

        if !self.has_sent() {
            return Ok(());
        }

        self.replace_track(None).await?;

        let track_encodings = self.track_encodings.lock().await;
        for encoding in track_encodings.iter() {
            let stream_info = encoding.stream_info.lock().await;
            self.interceptor.unbind_local_stream(&stream_info).await;

            encoding.srtp_stream.close().await?;

            if let Some(rtx) = &encoding.rtx {
                let rtx_stream_info = rtx.stream_info.lock().await;
                self.interceptor.unbind_local_stream(&rtx_stream_info).await;

                rtx.srtp_stream.close().await?;
            }
        }

        Ok(())
    }

    /// read reads incoming RTCP for this RTPReceiver
    pub async fn read(
        &self,
        b: &mut [u8],
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        tokio::select! {
            _ = self.wait_for_send() => {
                let rtcp_interceptor = {
                    let track_encodings = self.track_encodings.lock().await;
                    track_encodings.first().map(|e|e.rtcp_interceptor.clone())
                }.ok_or(Error::ErrInterceptorNotBind)?;
                let a = Attributes::new();
                tokio::select! {
                    _ = self.internal.stop_called_rx.notified() => Err(Error::ErrClosedPipe),
                    result = rtcp_interceptor.read(b, &a) => Ok(result?),
                }
            }
            _ = self.internal.stop_called_rx.notified() => Err(Error::ErrClosedPipe),
        }
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshals for you.
    pub async fn read_rtcp(
        &self,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        let mut b = vec![0u8; self.receive_mtu];
        let (pkts, attributes) = self.read(&mut b).await?;

        Ok((pkts, attributes))
    }

    /// ReadSimulcast reads incoming RTCP for this RTPSender for given rid
    pub async fn read_simulcast(
        &self,
        b: &mut [u8],
        rid: &str,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        tokio::select! {
            _ = self.wait_for_send() => {
                let rtcp_interceptor = {
                    let track_encodings = self.track_encodings.lock().await;
                    track_encodings.iter().find(|e| e.track.rid() == Some(rid)).map(|e| e.rtcp_interceptor.clone())
                }.ok_or(Error::ErrRTPSenderNoTrackForRID)?;
                let a = Attributes::new();
                tokio::select! {
                    _ = self.internal.stop_called_rx.notified() => Err(Error::ErrClosedPipe),
                    result = rtcp_interceptor.read(b, &a) => Ok(result?),
                }
            }
            _ = self.internal.stop_called_rx.notified() => Err(Error::ErrClosedPipe),
        }
    }

    /// ReadSimulcastRTCP is a convenience method that wraps ReadSimulcast and unmarshal for you
    pub async fn read_rtcp_simulcast(
        &self,
        rid: &str,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        let mut b = vec![0u8; self.receive_mtu];
        let (pkts, attributes) = self.read_simulcast(&mut b, rid).await?;

        Ok((pkts, attributes))
    }

    /// Enables overriding outgoing `RTP` packets' `sequence number`s.
    ///
    /// Must be called once before any data sent or never called at all.
    ///
    /// # Errors
    ///
    /// Errors if this [`RTCRtpSender`] has started to send data or sequence
    /// transforming has been already enabled.
    pub fn enable_seq_transformer(&self) -> Result<()> {
        self.seq_trans.enable()?;
        self.rtx_seq_trans.enable()
    }

    /// Will asynchronously block/wait until send() has been called
    ///
    /// Note that it could return if underlying channel is closed,
    /// however this shouldn't happen as we have a reference to self
    /// which again owns the underlying channel.
    pub async fn wait_for_send(&self) {
        let mut watch = self.send_called.subscribe();
        let _ = watch.wait_for(|r| *r).await;
    }

    /// has_sent tells if data has been ever sent for this instance
    pub(crate) fn has_sent(&self) -> bool {
        *self.send_called.borrow()
    }

    /// has_stopped tells if stop has been called
    pub(crate) async fn has_stopped(&self) -> bool {
        self.stop_called_signal.load(Ordering::SeqCst)
    }

    pub(crate) fn initial_track_id(&self) -> Option<String> {
        let lock = self.initial_track_id.lock().unwrap();

        lock.clone()
    }

    pub(crate) fn set_initial_track_id(&self, id: String) -> Result<()> {
        let mut lock = self.initial_track_id.lock().unwrap();

        if lock.is_some() {
            return Err(Error::ErrSenderInitialTrackIdAlreadySet);
        }

        *lock = Some(id);

        Ok(())
    }

    pub(crate) fn associate_media_stream_id(&self, id: String) -> bool {
        let mut lock = self.associated_media_stream_ids.lock().unwrap();

        if lock.contains(&id) {
            return false;
        }

        lock.push(id);

        true
    }

    pub(crate) fn associated_media_stream_ids(&self) -> Vec<String> {
        let lock = self.associated_media_stream_ids.lock().unwrap();

        lock.clone()
    }
}
