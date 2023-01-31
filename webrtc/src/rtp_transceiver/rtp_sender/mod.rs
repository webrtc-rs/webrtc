#[cfg(test)]
mod rtp_sender_test;

use crate::api::media_engine::MediaEngine;
use crate::dtls_transport::RTCDtlsTransport;
use crate::error::{Error, Result};
use crate::rtp_transceiver::rtp_codec::{RTCRtpCodecParameters, RTPCodecType};
use crate::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use crate::rtp_transceiver::srtp_writer_future::SrtpWriterFuture;
use crate::rtp_transceiver::{
    create_stream_info, PayloadType, RTCRtpEncodingParameters, RTCRtpSendParameters,
    RTCRtpTransceiver, SSRC,
};
use crate::track::track_local::{
    InterceptorToTrackLocalWriter, TrackLocal, TrackLocalContext, TrackLocalWriter,
};

use ice::rand::generate_crypto_random_string;
use interceptor::stream_info::StreamInfo;
use interceptor::{Attributes, Interceptor, RTCPReader, RTPWriter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::{mpsc, Mutex, Notify};

use super::srtp_writer_future::SequenceTransformer;

pub(crate) struct RTPSenderInternal {
    pub(crate) send_called_rx: Mutex<mpsc::Receiver<()>>,
    pub(crate) stop_called_rx: Arc<Notify>,
    pub(crate) stop_called_signal: Arc<AtomicBool>,
    pub(crate) rtcp_interceptor: Mutex<Option<Arc<dyn RTCPReader + Send + Sync>>>,
}

impl RTPSenderInternal {
    /// read reads incoming RTCP for this RTPReceiver
    async fn read(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        let mut send_called_rx = self.send_called_rx.lock().await;

        tokio::select! {
            _ = send_called_rx.recv() =>{
                let rtcp_interceptor = {
                    let rtcp_interceptor = self.rtcp_interceptor.lock().await;
                    rtcp_interceptor.clone()
                };
                if let Some(rtcp_interceptor) = rtcp_interceptor{
                    let a = Attributes::new();
                    tokio::select! {
                        _ = self.stop_called_rx.notified() => {
                            Err(Error::ErrClosedPipe)
                        }
                        result = rtcp_interceptor.read(b, &a) => {
                            Ok(result?)
                        }
                    }
                }else{
                    Err(Error::ErrInterceptorNotBind)
                }
            }
            _ = self.stop_called_rx.notified() =>{
                Err(Error::ErrClosedPipe)
            }
        }
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshals for you.
    async fn read_rtcp(
        &self,
        receive_mtu: usize,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        let mut b = vec![0u8; receive_mtu];
        let (n, attributes) = self.read(&mut b).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }
}

/// RTPSender allows an application to control how a given Track is encoded and transmitted to a remote peer
pub struct RTCRtpSender {
    pub(crate) track: Mutex<Option<Arc<dyn TrackLocal + Send + Sync>>>,

    pub(crate) srtp_stream: Arc<SrtpWriterFuture>,
    pub(crate) stream_info: Mutex<StreamInfo>,
    seq_trans: Arc<SequenceTransformer>,

    pub(crate) context: Mutex<TrackLocalContext>,

    pub(crate) transport: Arc<RTCDtlsTransport>,

    pub(crate) payload_type: PayloadType,
    pub(crate) ssrc: SSRC,
    receive_mtu: usize,

    /// a transceiver sender since we can just check the
    /// transceiver negotiation status
    pub(crate) negotiated: AtomicBool,

    pub(crate) media_engine: Arc<MediaEngine>,
    pub(crate) interceptor: Arc<dyn Interceptor + Send + Sync>,

    pub(crate) id: String,

    /// The id of the initial track, even if we later change to a different
    /// track id should be use when negotiating.
    pub(crate) initial_track_id: std::sync::Mutex<Option<String>>,
    /// AssociatedMediaStreamIds from the WebRTC specifcations
    pub(crate) associated_media_stream_ids: std::sync::Mutex<Vec<String>>,

    rtp_transceiver: Mutex<Option<Weak<RTCRtpTransceiver>>>,

    send_called_tx: Mutex<Option<mpsc::Sender<()>>>,
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
        receive_mtu: usize,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
        transport: Arc<RTCDtlsTransport>,
        media_engine: Arc<MediaEngine>,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
        start_paused: bool,
    ) -> Self {
        let id = generate_crypto_random_string(
            32,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
        );
        let (send_called_tx, send_called_rx) = mpsc::channel(1);
        let stop_called_tx = Arc::new(Notify::new());
        let stop_called_rx = stop_called_tx.clone();
        let ssrc = rand::random::<u32>();
        let stop_called_signal = Arc::new(AtomicBool::new(false));

        let internal = Arc::new(RTPSenderInternal {
            send_called_rx: Mutex::new(send_called_rx),
            stop_called_rx,
            stop_called_signal: Arc::clone(&stop_called_signal),
            rtcp_interceptor: Mutex::new(None),
        });

        let seq_trans = Arc::new(SequenceTransformer::new());
        let srtp_stream = Arc::new(SrtpWriterFuture {
            closed: AtomicBool::new(false),
            ssrc,
            rtp_sender: Arc::downgrade(&internal),
            rtp_transport: Arc::clone(&transport),
            rtcp_read_stream: Mutex::new(None),
            rtp_write_session: Mutex::new(None),
            seq_trans: Arc::clone(&seq_trans),
        });

        let srtp_rtcp_reader = Arc::clone(&srtp_stream) as Arc<dyn RTCPReader + Send + Sync>;
        let rtcp_interceptor = interceptor.bind_rtcp_reader(srtp_rtcp_reader).await;
        {
            let mut internal_rtcp_interceptor = internal.rtcp_interceptor.lock().await;
            *internal_rtcp_interceptor = Some(rtcp_interceptor);
        }

        let stream_ids = track
            .as_ref()
            .map(|track| vec![track.stream_id().to_string()])
            .unwrap_or_default();
        Self {
            track: Mutex::new(track),

            srtp_stream,
            stream_info: Mutex::new(StreamInfo::default()),
            seq_trans,

            context: Mutex::new(TrackLocalContext::default()),
            transport,

            payload_type: 0,
            ssrc,
            receive_mtu,

            negotiated: AtomicBool::new(false),

            media_engine,
            interceptor,

            id,
            initial_track_id: std::sync::Mutex::new(None),
            associated_media_stream_ids: std::sync::Mutex::new(stream_ids),

            rtp_transceiver: Mutex::new(None),

            send_called_tx: Mutex::new(Some(send_called_tx)),
            stop_called_tx,
            stop_called_signal,

            paused: Arc::new(AtomicBool::new(start_paused)),

            internal,
        }
    }

    pub(crate) fn is_negotiated(&self) -> bool {
        self.negotiated.load(Ordering::SeqCst)
    }

    pub(crate) fn set_negotiated(&self) {
        self.negotiated.store(true, Ordering::SeqCst);
    }

    pub(crate) async fn set_rtp_transceiver(
        &self,
        rtp_transceiver: Option<Weak<RTCRtpTransceiver>>,
    ) {
        if let Some(t) = rtp_transceiver.as_ref().and_then(|t| t.upgrade()) {
            self.set_paused(!t.direction().has_send());
        }
        let mut tr = self.rtp_transceiver.lock().await;
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
        let kind = {
            let track = self.track.lock().await;
            if let Some(t) = &*track {
                t.kind()
            } else {
                RTPCodecType::default()
            }
        };

        let mut send_parameters = {
            RTCRtpSendParameters {
                rtp_parameters: self
                    .media_engine
                    .get_rtp_parameters_by_kind(kind, RTCRtpTransceiverDirection::Sendonly)
                    .await,
                encodings: vec![RTCRtpEncodingParameters {
                    ssrc: self.ssrc,
                    payload_type: self.payload_type,
                    ..Default::default()
                }],
            }
        };

        let codecs = {
            let tr = self.rtp_transceiver.lock().await;
            if let Some(t) = &*tr {
                if let Some(t) = t.upgrade() {
                    t.get_codecs().await
                } else {
                    self.media_engine.get_codecs_by_kind(kind).await
                }
            } else {
                self.media_engine.get_codecs_by_kind(kind).await
            }
        };
        send_parameters.rtp_parameters.codecs = codecs;

        send_parameters
    }

    /// track returns the RTCRtpTransceiver track, or nil
    pub async fn track(&self) -> Option<Arc<dyn TrackLocal + Send + Sync>> {
        let track = self.track.lock().await;
        track.clone()
    }

    /// replace_track replaces the track currently being used as the sender's source with a new TrackLocal.
    /// The new track must be of the same media kind (audio, video, etc) and switching the track should not
    /// require negotiation.
    pub async fn replace_track(
        &self,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        if let Some(t) = &track {
            let tr = self.rtp_transceiver.lock().await;
            if let Some(r) = &*tr {
                if let Some(r) = r.upgrade() {
                    if r.kind != t.kind() {
                        return Err(Error::ErrRTPSenderNewTrackHasIncorrectKind);
                    }
                } else {
                    //TODO: what about None arc?
                }
            } else {
                //TODO: what about None tr?
            }
        }

        if self.has_sent().await {
            let t = {
                let t = self.track.lock().await;
                t.clone()
            };
            if let Some(t) = t {
                let context = self.context.lock().await;
                t.unbind(&context).await?;
            }
        }

        if !self.has_sent().await || track.is_none() {
            let mut t = self.track.lock().await;
            *t = track;
            return Ok(());
        }

        let context = {
            let context = self.context.lock().await;
            context.clone()
        };

        let result = if let Some(t) = &track {
            self.seq_trans.reset_offset();

            let new_context = TrackLocalContext {
                id: context.id.clone(),
                params: self
                    .media_engine
                    .get_rtp_parameters_by_kind(t.kind(), RTCRtpTransceiverDirection::Sendonly)
                    .await,
                ssrc: context.ssrc,
                write_stream: context.write_stream.clone(),
                paused: self.paused.clone(),
            };

            t.bind(&new_context).await
        } else {
            Err(Error::ErrRTPSenderTrackNil)
        };

        match result {
            Err(err) => {
                // Re-bind the original track
                let track = self.track.lock().await;
                if let Some(t) = &*track {
                    t.bind(&context).await?;
                }

                Err(err)
            }
            Ok(codec) => {
                // Codec has changed
                if self.payload_type != codec.payload_type {
                    let mut context = self.context.lock().await;
                    context.params.codecs = vec![codec];
                }

                {
                    let mut t = self.track.lock().await;
                    *t = track;
                }

                Ok(())
            }
        }
    }

    /// send Attempts to set the parameters controlling the sending of media.
    pub async fn send(&self, parameters: &RTCRtpSendParameters) -> Result<()> {
        if self.has_sent().await {
            return Err(Error::ErrRTPSenderSendAlreadyCalled);
        }

        let write_stream = Arc::new(InterceptorToTrackLocalWriter::new(self.paused.clone()));
        let (context, stream_info) = {
            let track = self.track.lock().await;
            let mut context = TrackLocalContext {
                id: self.id.clone(),
                params: self
                    .media_engine
                    .get_rtp_parameters_by_kind(
                        if let Some(t) = &*track {
                            t.kind()
                        } else {
                            RTPCodecType::default()
                        },
                        RTCRtpTransceiverDirection::Sendonly,
                    )
                    .await,
                ssrc: parameters.encodings[0].ssrc,
                write_stream: Some(
                    Arc::clone(&write_stream) as Arc<dyn TrackLocalWriter + Send + Sync>
                ),
                paused: self.paused.clone(),
            };

            let codec = if let Some(t) = &*track {
                t.bind(&context).await?
            } else {
                RTCRtpCodecParameters::default()
            };
            let payload_type = codec.payload_type;
            let capability = codec.capability.clone();
            context.params.codecs = vec![codec];
            let stream_info = create_stream_info(
                self.id.clone(),
                parameters.encodings[0].ssrc,
                payload_type,
                capability,
                &parameters.rtp_parameters.header_extensions,
            );

            (context, stream_info)
        };

        let srtp_rtp_writer = Arc::clone(&self.srtp_stream) as Arc<dyn RTPWriter + Send + Sync>;
        let rtp_interceptor = self
            .interceptor
            .bind_local_stream(&stream_info, srtp_rtp_writer)
            .await;
        {
            let mut interceptor_rtp_writer = write_stream.interceptor_rtp_writer.lock().await;
            *interceptor_rtp_writer = Some(rtp_interceptor);
        }

        {
            let mut ctx = self.context.lock().await;
            *ctx = context;
        }
        {
            let mut si = self.stream_info.lock().await;
            *si = stream_info;
        }

        {
            let mut send_called_tx = self.send_called_tx.lock().await;
            send_called_tx.take();
        }

        Ok(())
    }

    /// stop irreversibly stops the RTPSender
    pub async fn stop(&self) -> Result<()> {
        if self.stop_called_signal.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.stop_called_signal.store(true, Ordering::SeqCst);
        self.stop_called_tx.notify_waiters();

        if !self.has_sent().await {
            return Ok(());
        }

        self.replace_track(None).await?;

        {
            let stream_info = self.stream_info.lock().await;
            self.interceptor.unbind_local_stream(&stream_info).await;
        }

        self.srtp_stream.close().await
    }

    /// read reads incoming RTCP for this RTPReceiver
    pub async fn read(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        self.internal.read(b).await
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshals for you.
    pub async fn read_rtcp(
        &self,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        self.internal.read_rtcp(self.receive_mtu).await
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
        self.seq_trans.enable()
    }

    /// has_sent tells if data has been ever sent for this instance
    pub(crate) async fn has_sent(&self) -> bool {
        let send_called_tx = self.send_called_tx.lock().await;
        send_called_tx.is_none()
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
