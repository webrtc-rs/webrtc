#[cfg(test)]
mod rtp_sender_test;

use crate::api::media_engine::MediaEngine;
use crate::error::Error;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::interceptor::{create_stream_info, InterceptorToTrackLocalWriter};
use crate::media::rtp::rtp_codec::{RTPCodecParameters, RTPCodecType};
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::rtp::srtp_writer_future::SrtpWriterFuture;
use crate::media::rtp::{PayloadType, RTPEncodingParameters, RTPSendParameters, SSRC};
use crate::media::track::track_local::{TrackLocal, TrackLocalContext};
use crate::RECEIVE_MTU;

use anyhow::Result;
use ice::rand::generate_crypto_random_string;
use interceptor::stream_info::StreamInfo;
use interceptor::{Attributes, Interceptor, RTCPReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

pub(crate) struct RTPSenderInternal {
    pub(crate) send_called_rx: mpsc::Receiver<()>,
    pub(crate) stop_called_rx: mpsc::Receiver<()>,
    pub(crate) rtcp_interceptor: Option<Box<dyn RTCPReader + Send + Sync>>,
}

impl RTPSenderInternal {
    /// read reads incoming RTCP for this RTPReceiver
    async fn read(&mut self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        tokio::select! {
            _ = self.send_called_rx.recv() =>{
                if let Some(rtcp_interceptor) = &self.rtcp_interceptor{
                    let a = Attributes::new();
                    rtcp_interceptor.read(b, &a).await
                }else{
                    Err(Error::ErrInterceptorNotBind.into())
                }
            }
            _ = self.stop_called_rx.recv() =>{
                Err(Error::ErrClosedPipe.into())
            }
        }
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshals for you.
    async fn read_rtcp(&mut self) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, attributes) = self.read(&mut b).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }
}

/// RTPSender allows an application to control how a given Track is encoded and transmitted to a remote peer
pub struct RTPSender {
    pub(crate) track: Mutex<Option<Arc<dyn TrackLocal + Send + Sync>>>,

    pub(crate) srtp_stream: SrtpWriterFuture,
    pub(crate) stream_info: Mutex<StreamInfo>,

    pub(crate) context: Mutex<TrackLocalContext>,

    pub(crate) transport: Arc<DTLSTransport>,

    pub(crate) payload_type: PayloadType,
    pub(crate) ssrc: SSRC,

    /// a transceiver sender since we can just check the
    /// transceiver negotiation status
    pub(crate) negotiated: AtomicBool,

    pub(crate) media_engine: Arc<MediaEngine>,
    pub(crate) interceptor: Arc<dyn Interceptor + Send + Sync>,

    pub(crate) id: String,

    send_called_tx: Mutex<Option<mpsc::Sender<()>>>,
    stop_called_tx: Mutex<Option<mpsc::Sender<()>>>,

    internal: Arc<Mutex<RTPSenderInternal>>,
}

impl RTPSender {
    pub fn new(
        track: Arc<dyn TrackLocal + Send + Sync>,
        transport: Arc<DTLSTransport>,
        media_engine: Arc<MediaEngine>,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
    ) -> RTPSender {
        let id = generate_crypto_random_string(
            32,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
        );
        let (send_called_tx, send_called_rx) = mpsc::channel(1);
        let (stop_called_tx, stop_called_rx) = mpsc::channel(1);
        let ssrc = rand::random::<u32>();

        /*TODO: r.rtcp_interceptor = r.api.interceptor.bind_rtcpreader(interceptor.RTPReaderFunc(func(in []byte, a interceptor.Attributes) (n int, attributes interceptor.Attributes, err error) {
            n, err = r.srtp_stream.Read(in)
            return n, a, err
        }))*/
        let internal = Arc::new(Mutex::new(RTPSenderInternal {
            send_called_rx,
            stop_called_rx,
            rtcp_interceptor: None,
        }));

        let srtp_stream = SrtpWriterFuture {
            ssrc,
            rtp_sender: Arc::clone(&internal),
            rtp_transport: Arc::clone(&transport),
            rtcp_read_stream: Mutex::new(None),
            rtp_write_session: Mutex::new(None),
        };

        RTPSender {
            track: Mutex::new(Some(track)),

            srtp_stream,
            stream_info: Mutex::new(StreamInfo::default()),

            context: Mutex::new(TrackLocalContext::default()),
            transport,

            payload_type: 0,
            ssrc,

            negotiated: AtomicBool::new(false),

            media_engine,
            interceptor,

            id,

            send_called_tx: Mutex::new(Some(send_called_tx)),
            stop_called_tx: Mutex::new(Some(stop_called_tx)),

            internal,
        }
    }

    pub(crate) fn is_negotiated(&self) -> bool {
        self.negotiated.load(Ordering::SeqCst)
    }

    pub(crate) fn set_negotiated(&self) {
        self.negotiated.store(true, Ordering::SeqCst);
    }

    /// transport returns the currently-configured DTLSTransport
    /// if one has not yet been configured
    pub fn transport(&self) -> Arc<DTLSTransport> {
        Arc::clone(&self.transport)
    }

    /// get_parameters describes the current configuration for the encoding and
    /// transmission of media on the sender's track.
    pub async fn get_parameters(&self) -> RTPSendParameters {
        let track = self.track.lock().await;
        RTPSendParameters {
            rtp_parameters: self
                .media_engine
                .get_rtp_parameters_by_kind(
                    if let Some(t) = &*track {
                        t.kind()
                    } else {
                        RTPCodecType::default()
                    },
                    &[RTPTransceiverDirection::Sendonly],
                )
                .await,
            encodings: vec![RTPEncodingParameters {
                rid: String::new(),
                ssrc: self.ssrc,
                payload_type: self.payload_type,
            }],
        }
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
        if self.has_sent().await {
            let t = self.track.lock().await;
            if let Some(track) = &*t {
                let context = self.context.lock().await;
                track.unbind(&*context).await?;
            }
        }

        if !self.has_sent().await || track.is_none() {
            let mut t = self.track.lock().await;
            *t = track;
            return Ok(());
        }

        let result = if let Some(t) = &track {
            // Re-bind the original track
            let context = self.context.lock().await;
            t.bind(&*context).await
        } else {
            Err(Error::ErrRTPSenderTrackNil.into())
        };

        if let Err(err) = result {
            return Err(err);
        }

        let mut t = self.track.lock().await;
        *t = track;

        Ok(())
    }

    /// send Attempts to set the parameters controlling the sending of media.
    pub async fn send(&self, parameters: &RTPSendParameters) -> Result<()> {
        if self.has_sent().await {
            return Err(Error::ErrRTPSenderSendAlreadyCalled.into());
        }

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
                        &[RTPTransceiverDirection::Sendonly],
                    )
                    .await,
                ssrc: parameters.encodings[0].ssrc,
                write_stream: Some(Box::new(InterceptorToTrackLocalWriter {})),
            };

            let codec = if let Some(t) = &*track {
                t.bind(&context).await?
            } else {
                RTPCodecParameters::default()
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

        {
            let mut ctx = self.context.lock().await;
            *ctx = context;
        }
        {
            let mut si = self.stream_info.lock().await;
            *si = stream_info;
        }

        /*TODO: rtpInterceptor := r.api.interceptor.bind_local_stream(&r.stream_info, interceptor.RTPWriterFunc(func(header *rtp.Header, payload []byte, attributes interceptor.Attributes) (int, error) {
            return r.srtp_stream.write_rtp(header, payload)
        }))
        writeStream.interceptor.Store(rtpInterceptor)*/

        {
            let mut send_called_tx = self.send_called_tx.lock().await;
            send_called_tx.take();
        }

        Ok(())
    }

    /// stop irreversibly stops the RTPSender
    pub async fn stop(&self) -> Result<()> {
        {
            let mut stop_called_tx = self.stop_called_tx.lock().await;
            if stop_called_tx.is_none() {
                return Ok(());
            }
            stop_called_tx.take();
        }

        if !self.has_sent().await {
            return Ok(());
        }

        self.replace_track(None).await?;

        {
            let stream_info = self.stream_info.lock().await;
            self.interceptor.unbind_local_stream(&*stream_info).await;
        }

        self.srtp_stream.close().await
    }

    /// read reads incoming RTCP for this RTPReceiver
    pub async fn read(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        let mut internal = self.internal.lock().await;
        internal.read(b).await
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshals for you.
    pub async fn read_rtcp(&self) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut internal = self.internal.lock().await;
        internal.read_rtcp().await
    }

    /// has_sent tells if data has been ever sent for this instance
    pub(crate) async fn has_sent(&self) -> bool {
        let send_called_tx = self.send_called_tx.lock().await;
        send_called_tx.is_none()
    }

    /// has_stopped tells if stop has been called
    pub(crate) async fn has_stopped(&self) -> bool {
        let stop_called_tx = self.stop_called_tx.lock().await;
        stop_called_tx.is_none()
    }
}
