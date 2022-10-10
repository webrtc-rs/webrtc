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

pub(crate) struct RTPSenderInternal {
    pub(crate) send_called_rx: Mutex<mpsc::Receiver<()>>,
    pub(crate) stop_called_rx: Arc<Notify>,
    pub(crate) stop_called_signal: Arc<AtomicBool>,
}

impl RTPSenderInternal {
    /// read reads incoming RTCP for this RTPReceiver
    async fn read(
        &self,
        encoding: &Arc<TrackEncoding>,
        b: &mut [u8],
    ) -> Result<(usize, Attributes)> {
        let mut send_called_rx = self.send_called_rx.lock().await;

        tokio::select! {
            _ = send_called_rx.recv() =>{
                let rtcp_interceptor = encoding.rtcp_interceptor.clone();
                    let a = Attributes::new();
                    tokio::select! {
                        _ = self.stop_called_rx.notified() => {
                            Err(Error::ErrClosedPipe)
                        }
                        result = rtcp_interceptor.read(b, &a) => {
                            Ok(result?)
                        }
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
        encoding: &Arc<TrackEncoding>,
        receive_mtu: usize,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        let mut b = vec![0u8; receive_mtu];
        let (n, attributes) = self.read(encoding, &mut b).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }
}

pub struct TrackEncoding {
    pub(crate) track: Mutex<Option<Arc<dyn TrackLocal + Send + Sync>>>,

    pub(crate) srtp_stream: Arc<SrtpWriterFuture>,

    pub(crate) rtcp_interceptor: Arc<dyn RTCPReader + Send + Sync>,
    pub(crate) stream_info: Mutex<StreamInfo>,

    pub(crate) context: Mutex<TrackLocalContext>,

    pub(crate) ssrc: SSRC,
}

/// RTPSender allows an application to control how a given Track is encoded and transmitted to a remote peer
pub struct RTCRtpSender {
    pub(crate) track_encodings: Mutex<Vec<Arc<TrackEncoding>>>,

    pub(crate) transport: Arc<RTCDtlsTransport>,

    pub(crate) payload_type: PayloadType,
    receive_mtu: usize,

    /// a transceiver sender since we can just check the
    /// transceiver negotiation status
    pub(crate) negotiated: AtomicBool,

    pub(crate) media_engine: Arc<MediaEngine>,
    pub(crate) interceptor: Arc<dyn Interceptor + Send + Sync>,
    pub(crate) kind: RTPCodecType,

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
        track: Arc<dyn TrackLocal + Send + Sync>,
        transport: Arc<RTCDtlsTransport>,
        media_engine: Arc<MediaEngine>,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
        start_paused: bool,
    ) -> RTCRtpSender {
        let id = generate_crypto_random_string(
            32,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
        );
        let (send_called_tx, send_called_rx) = mpsc::channel(1);
        let stop_called_tx = Arc::new(Notify::new());
        let stop_called_rx = stop_called_tx.clone();
        let stop_called_signal = Arc::new(AtomicBool::new(false));
        let stream_ids = vec![track.stream_id().to_string()];

        let internal = Arc::new(RTPSenderInternal {
            send_called_rx: Mutex::new(send_called_rx),
            stop_called_rx,
            stop_called_signal: Arc::clone(&stop_called_signal),
        });

        let sender = RTCRtpSender {
            track_encodings: Mutex::new(vec![]),
            transport,

            payload_type: 0,
            receive_mtu,

            negotiated: AtomicBool::new(false),

            media_engine,
            interceptor,

            id,
            initial_track_id: std::sync::Mutex::new(None),
            associated_media_stream_ids: std::sync::Mutex::new(stream_ids),
            kind: track.kind(),

            rtp_transceiver: Mutex::new(None),

            send_called_tx: Mutex::new(Some(send_called_tx)),
            stop_called_tx,
            stop_called_signal,

            paused: Arc::new(AtomicBool::new(start_paused)),

            internal,
        };

        sender.add_encoding_internal(track).await;

        // Add track
        sender
    }

    pub async fn add_encoding(&self, track: Arc<dyn TrackLocal + Send + Sync>) -> Result<()> {
        if track.rid() == "" {
            return Err(Error::ErrRTPSenderRidNil);
        }

        if self.has_stopped().await {
            return Err(Error::ErrRTPSenderStopped);
        }

        if self.has_sent().await {
            return Err(Error::ErrRTPSenderSendAlreadyCalled);
        }

        // oops, somebody code-golf this for me
        {
            let ref_track = if let Some(t) = self.first_encoding().await? {
                let t = t.track.lock().await;
                if let Some(t) = &*t {
                    if t.rid() != "" {
                        t.clone()
                    } else {
                        return Err(Error::ErrRTPSenderNoBaseEncoding);
                    }
                } else {
                    return Err(Error::ErrRTPSenderNoBaseEncoding);
                }
            } else {
                return Err(Error::ErrRTPSenderNoBaseEncoding);
            };

            if ref_track.id() != track.id()
                || ref_track.stream_id() != track.stream_id()
                || ref_track.kind() != track.kind()
            {
                return Err(Error::ErrRTPSenderBaseEncodingMismatch);
            }

            if self.encoding_for_rid(track.rid()).await.is_some() {
                return Err(Error::ErrRTPSenderRIDCollision);
            }
        }

        self.add_encoding_internal(track).await;
        Ok(())
    }

    pub(crate) async fn add_encoding_internal(&self, track: Arc<dyn TrackLocal + Send + Sync>) {
        let ssrc = rand::random::<u32>();
        let srtp_stream = Arc::new(SrtpWriterFuture {
            closed: AtomicBool::new(false),
            ssrc,
            rtp_sender: Arc::downgrade(&self.internal),
            rtp_transport: Arc::clone(&self.transport),
            rtcp_read_stream: Mutex::new(None),
            rtp_write_session: Mutex::new(None),
        });

        let srtp_rtcp_reader = Arc::clone(&srtp_stream) as Arc<dyn RTCPReader + Send + Sync>;
        let rtcp_interceptor = self.interceptor.bind_rtcp_reader(srtp_rtcp_reader).await;

        let track_encoding = TrackEncoding {
            track: Mutex::new(Some(track)),
            srtp_stream,
            ssrc,
            rtcp_interceptor,
            stream_info: Mutex::new(StreamInfo::default()),
            context: Mutex::new(TrackLocalContext::default()),
        };

        let mut encodings = self.track_encodings.lock().await;
        encodings.push(Arc::new(track_encoding));
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
        let mut encodings: Vec<RTCRtpEncodingParameters> = vec![];

        {
            let track_encodings = self.track_encodings.lock().await;
            for te in track_encodings.iter() {
                let track = te.track.lock().await;
                let rid = track
                    .as_ref()
                    .map_or(String::from(""), |t| String::from(t.rid()));

                encodings.push(RTCRtpEncodingParameters {
                    ssrc: te.ssrc,
                    payload_type: self.payload_type,
                    rid,
                    ..Default::default()
                })
            }
        }

        let mut send_parameters = RTCRtpSendParameters {
            rtp_parameters: self
                .media_engine
                .get_rtp_parameters_by_kind(self.kind, &[RTCRtpTransceiverDirection::Sendonly])
                .await,
            encodings,
        };

        let codecs = {
            let tr = self.rtp_transceiver.lock().await;
            if let Some(t) = &*tr {
                if let Some(t) = t.upgrade() {
                    t.get_codecs().await
                } else {
                    self.media_engine.get_codecs_by_kind(self.kind).await
                }
            } else {
                self.media_engine.get_codecs_by_kind(self.kind).await
            }
        };
        send_parameters.rtp_parameters.codecs = codecs;

        send_parameters
    }

    /// track returns the RTCRtpTransceiver track, or nil
    pub async fn track(&self) -> Option<Arc<dyn TrackLocal + Send + Sync>> {
        let encodings = self.track_encodings.lock().await;
        if let Some(t) = encodings.first() {
            let track = t.track.lock().await;
            track.clone()
        } else {
            None
        }
    }

    /// replace_track replaces the track currently being used as the sender's source with a new TrackLocal.
    /// The new track must be of the same media kind (audio, video, etc) and switching the track should not
    /// require negotiation.
    pub async fn replace_track(
        &self,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        if let Some(t) = &track {
            let encodings = self.track_encodings.lock().await;
            if encodings.len() > 1 {
                // return ErrRTPSenderNewTrackHasIncorrectEnvelope
                return Err(Error::ErrRTPSenderNewTrackHasIncorrectKind);
            }
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

        let encodings = self.track_encodings.lock().await;
        if let Some(re) = encodings.first() {
            if self.has_sent().await {
                let t = {
                    let t = re.track.lock().await;
                    t.clone()
                };
                if let Some(t) = t {
                    let context = re.context.lock().await;
                    t.unbind(&context).await?;
                }
            }

            if !self.has_sent().await || track.is_none() {
                let mut t = re.track.lock().await;
                *t = track;
                return Ok(());
            }

            let context = {
                let context = re.context.lock().await;
                context.clone()
            };

            let result = if let Some(t) = &track {
                let new_context = TrackLocalContext {
                    id: context.id.clone(),
                    params: self
                        .media_engine
                        .get_rtp_parameters_by_kind(
                            t.kind(),
                            &[RTCRtpTransceiverDirection::Sendonly],
                        )
                        .await,
                    ssrc: context.ssrc,
                    write_stream: context.write_stream.clone(),
                    rtcp_intercepter: context.rtcp_intercepter.clone(),
                };

                t.bind(&new_context).await
            } else {
                Err(Error::ErrRTPSenderTrackNil)
            };

            match result {
                Err(err) => {
                    // Re-bind the original track
                    let track = re.track.lock().await;
                    if let Some(t) = &*track {
                        t.bind(&context).await?;
                    }

                    Err(err)
                }
                Ok(codec) => {
                    // Codec has changed
                    if self.payload_type != codec.payload_type {
                        let mut context = re.context.lock().await;
                        context.params.codecs = vec![codec];
                    }

                    {
                        let mut t = re.track.lock().await;
                        *t = track;
                    }

                    Ok(())
                }
            }
        } else {
            // Is it though?
            // How do we end up in a state where we don't have at the very least, the default track
            // encoding?
            Ok(())
        }
    }

    /// send Attempts to set the parameters controlling the sending of media.
    pub async fn send(&self, parameters: &RTCRtpSendParameters) -> Result<()> {
        if self.has_sent().await {
            return Err(Error::ErrRTPSenderSendAlreadyCalled);
        }

        // This is quite a long lived lock?
        let encodings = self.track_encodings.lock().await;
        for te in encodings.iter() {
            let write_stream = Arc::new(InterceptorToTrackLocalWriter::new(self.paused.clone()));
            let (context, stream_info) = {
                let track = te.track.lock().await;
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
                            &[RTCRtpTransceiverDirection::Sendonly],
                        )
                        .await,
                    ssrc: te.ssrc,
                    rtcp_intercepter: Some(te.rtcp_interceptor.clone()),
                    write_stream: Some(
                        Arc::clone(&write_stream) as Arc<dyn TrackLocalWriter + Send + Sync>
                    ),
                };

                let (codec, rid) = if let Some(t) = &*track {
                    let codec = t.bind(&context).await?;
                    (codec, t.rid())
                } else {
                    (RTCRtpCodecParameters::default(), "")
                };
                let payload_type = codec.payload_type;
                let capability = codec.capability.clone();
                context.params.codecs = vec![codec];
                let stream_info = create_stream_info(
                    self.id.clone(),
                    te.ssrc,
                    rid.to_owned(),
                    payload_type,
                    capability,
                    &parameters.rtp_parameters.header_extensions,
                );

                (context, stream_info)
            };

            let srtp_rtp_writer = Arc::clone(&te.srtp_stream) as Arc<dyn RTPWriter + Send + Sync>;
            let rtp_interceptor = self
                .interceptor
                .bind_local_stream(&stream_info, srtp_rtp_writer)
                .await;
            {
                let mut interceptor_rtp_writer = write_stream.interceptor_rtp_writer.lock().await;
                *interceptor_rtp_writer = Some(rtp_interceptor);
            }

            {
                let mut ctx = te.context.lock().await;
                *ctx = context;
            }
            {
                let mut si = te.stream_info.lock().await;
                *si = stream_info;
            }
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

        let encodings = self.track_encodings.lock().await;
        for te in encodings.iter() {
            let stream_info = te.stream_info.lock().await;
            self.interceptor.unbind_local_stream(&stream_info).await;
            te.srtp_stream.close().await?
        }
        Ok(())
    }

    /// read reads incoming RTCP for this RTPReceiver
    pub async fn read(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        if let Some(encoding) = self.first_encoding().await? {
            self.internal.read(&encoding, b).await
        } else {
            Err(Error::ErrInterceptorNotBind)
        }
    }

    // Having a mutex on that little collection sure does make this whole module fun
    // These helpers exist because otherwise I accidentally end up locking on blocking calls
    // because I'm incapable of writing threadsafe code
    async fn encoding_for_rid(&self, rid: &str) -> Option<Arc<TrackEncoding>> {
        let encodings = self.track_encodings.lock().await;
        for e in encodings.iter() {
            if let Some(track) = &*e.track.lock().await {
                if track.rid() == rid {
                    return Some(e.clone());
                }
            };
        }
        None
    }

    async fn first_encoding(&self) -> Result<Option<Arc<TrackEncoding>>> {
        let encodings = self.track_encodings.lock().await;
        return Ok(encodings.first().map(|x| (*x).clone()));
    }

    pub async fn read_simulcast(&self, b: &mut [u8], rid: &str) -> Result<(usize, Attributes)> {
        if let Some(encoding) = self.encoding_for_rid(rid).await {
            self.internal.read(&encoding, b).await
        } else {
            Err(Error::ErrInterceptorNotBind)
        }
    }

    pub async fn read_simulcast_rtcp(
        &self,
        rid: &str,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        if let Some(encoding) = self.encoding_for_rid(rid).await {
            self.internal.read_rtcp(&encoding, self.receive_mtu).await
        } else {
            Err(Error::ErrInterceptorNotBind)
        }
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshals for you.
    pub async fn read_rtcp(
        &self,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        if let Some(encoding) = self.first_encoding().await? {
            self.internal.read_rtcp(&encoding, self.receive_mtu).await
        } else {
            Err(Error::ErrInterceptorNotBind)
        }
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

    pub(crate) fn associated_media_stream_ids(&self) -> Vec<String> {
        let lock = self.associated_media_stream_ids.lock().unwrap();

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
}
