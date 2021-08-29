use crate::api::media_engine::MediaEngine;
use crate::error::Error;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::interceptor::stream_info::StreamInfo;
use crate::media::interceptor::*;
use crate::media::rtp::rtp_codec::{RTPCodecCapability, RTPCodecType, RTPParameters};
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::rtp::{RTPReceiveParameters, SSRC};
use crate::media::track::track_remote::TrackRemote;
use crate::media::track::TrackStreams;
use crate::util::flatten_errs;
use crate::RECEIVE_MTU;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

struct RTPReceiverInternal {
    kind: RTPCodecType,
    tracks: Vec<TrackStreams>,

    transport: Arc<DTLSTransport>,
    media_engine: Arc<MediaEngine>,
    interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,

    closed_tx: Option<mpsc::Sender<()>>,
    closed_rx: mpsc::Receiver<()>,
    received_tx: Option<mpsc::Sender<()>>,
    received_rx: mpsc::Receiver<()>,
}

impl RTPReceiverInternal {
    /// receive initialize the track and starts all the transports
    async fn receive(&mut self, parameters: &RTPReceiveParameters) -> Result<()> {
        tokio::select! {
            _ = self.received_rx.recv() => {
                return Err(Error::ErrRTPReceiverReceiveAlreadyCalled.into());
            }
            else => {}  // default:
        };
        let _d = self.received_tx.take(); // defer drop(received_tx)

        if parameters.encodings.len() == 1 && parameters.encodings[0].ssrc != 0 {
            if let Some(encoding) = parameters.encodings.first() {
                let global_params = self.get_parameters().await;
                let codec = if let Some(codec) = global_params.codecs.first() {
                    codec.capability.clone()
                } else {
                    RTPCodecCapability::default()
                };

                let stream_info = StreamInfo::new(
                    "".to_owned(),
                    encoding.ssrc,
                    0,
                    codec,
                    &global_params.header_extensions,
                );
                let (rtp_read_stream, rtp_interceptor, rtcp_read_stream, rtcp_interceptor) =
                    RTPReceiver::streams_for_ssrc(&self.transport, encoding.ssrc, &stream_info)
                        .await?;

                let t = TrackStreams {
                    track: Arc::new(TrackRemote::new(
                        self.kind,
                        encoding.ssrc,
                        "".to_owned(),
                        Arc::clone(&self.media_engine),
                        self.interceptor.clone(),
                    )),
                    stream_info,
                    rtp_read_stream,
                    rtp_interceptor,
                    rtcp_read_stream,
                    rtcp_interceptor,
                };

                self.tracks.push(t);
            }
        } else {
            for encoding in &parameters.encodings {
                self.tracks.push(TrackStreams {
                    track: Arc::new(TrackRemote::new(
                        self.kind,
                        0,
                        encoding.rid.clone(),
                        Arc::clone(&self.media_engine),
                        self.interceptor.clone(),
                    )),
                    ..Default::default()
                });
            }
        }

        Ok(())
    }

    /// read reads incoming RTCP for this RTPReceiver
    async fn read(&mut self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        tokio::select! {
            _ = self.received_rx.recv() =>{
                if let Some(t) = self.tracks.first(){
                    if let Some(rtcp_interceptor) = &t.rtcp_interceptor{
                        let a = Attributes::new();
                        rtcp_interceptor.read(b, &a).await
                    }else{
                        Err(Error::ErrInterceptorNotBind.into())
                    }
                }else{
                    Err(Error::ErrExistingTrack.into())
                }
            }
            _ = self.closed_rx.recv() => {
                Err(Error::ErrClosedPipe.into())
            }
        }
    }

    /// read_simulcast reads incoming RTCP for this RTPReceiver for given rid
    async fn read_simulcast(&mut self, b: &mut [u8], rid: &str) -> Result<(usize, Attributes)> {
        tokio::select! {
            _ = self.received_rx.recv() =>{
                for t in &self.tracks{
                    if t.track.rid() == rid {
                       if let Some(rtcp_interceptor) = &t.rtcp_interceptor{
                            let a = Attributes::new();
                            return rtcp_interceptor.read(b, &a).await;
                        }else{
                            return Err(Error::ErrInterceptorNotBind.into());
                        }
                    }
                }
                Err(Error::ErrRTPReceiverForRIDTrackStreamNotFound.into())
            }
            _ = self.closed_rx.recv() => {
                Err(Error::ErrClosedPipe.into())
            }
        }
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshal for you.
    /// It also runs any configured interceptors.
    async fn read_rtcp(&mut self) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, attributes) = self.read(&mut b).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }

    /// read_simulcast_rtcp is a convenience method that wraps ReadSimulcast and unmarshal for you
    async fn read_simulcast_rtcp(
        &mut self,
        rid: &str,
    ) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, attributes) = self.read_simulcast(&mut b, rid).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }

    async fn read_rtp(&mut self, b: &mut [u8], tid: &str) -> Result<(usize, Attributes)> {
        let _ = self.received_rx.recv().await;

        for t in &self.tracks {
            if t.track.id() == tid {
                if let Some(ri) = &t.rtp_interceptor {
                    let a = Attributes::new();
                    return ri.read(b, &a).await;
                }
            }
        }

        Err(Error::ErrRTPReceiverWithSSRCTrackStreamNotFound.into())
    }

    async fn get_parameters(&self) -> RTPParameters {
        self.media_engine
            .get_rtp_parameters_by_kind(self.kind, &[RTPTransceiverDirection::Recvonly])
            .await
    }
}

/// RTPReceiver allows an application to inspect the receipt of a TrackRemote
pub struct RTPReceiver {
    kind: RTPCodecType,
    transport: Arc<DTLSTransport>,
    internal: Mutex<RTPReceiverInternal>,
}

impl RTPReceiver {
    pub fn new(
        kind: RTPCodecType,
        transport: Arc<DTLSTransport>,
        media_engine: Arc<MediaEngine>,
        interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,
    ) -> Self {
        let (closed_tx, closed_rx) = mpsc::channel(1);
        let (received_tx, received_rx) = mpsc::channel(1);

        RTPReceiver {
            kind,
            transport: Arc::clone(&transport),
            internal: Mutex::new(RTPReceiverInternal {
                kind,

                tracks: vec![],
                transport,
                media_engine,
                interceptor,

                closed_tx: Some(closed_tx),
                closed_rx,
                received_tx: Some(received_tx),
                received_rx,
            }),
        }
    }

    pub fn kind(&self) -> RTPCodecType {
        self.kind
    }

    /// transport returns the currently-configured *DTLSTransport or nil
    /// if one has not yet been configured
    pub fn transport(&self) -> Arc<DTLSTransport> {
        Arc::clone(&self.transport)
    }

    /// get_parameters describes the current configuration for the encoding and
    /// transmission of media on the receiver's track.
    pub async fn get_parameters(&self) -> RTPParameters {
        let internal = self.internal.lock().await;
        internal.get_parameters().await
    }

    /// track returns the RtpTransceiver TrackRemote
    pub async fn track(&self) -> Option<Arc<TrackRemote>> {
        let internal = self.internal.lock().await;
        internal.tracks.first().map(|t| Arc::clone(&t.track))
    }

    /// tracks returns the RtpTransceiver tracks
    /// A RTPReceiver to support Simulcast may now have multiple tracks
    pub async fn tracks(&self) -> Vec<Arc<TrackRemote>> {
        let internal = self.internal.lock().await;
        internal
            .tracks
            .iter()
            .map(|t| Arc::clone(&t.track))
            .collect()
    }

    /// receive initialize the track and starts all the transports
    pub async fn receive(&self, parameters: &RTPReceiveParameters) -> Result<()> {
        let mut internal = self.internal.lock().await;
        internal.receive(parameters).await
    }

    /// read reads incoming RTCP for this RTPReceiver
    pub async fn read(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        let mut internal = self.internal.lock().await;
        internal.read(b).await
    }

    /// read_simulcast reads incoming RTCP for this RTPReceiver for given rid
    pub async fn read_simulcast(&self, b: &mut [u8], rid: &str) -> Result<(usize, Attributes)> {
        let mut internal = self.internal.lock().await;
        internal.read_simulcast(b, rid).await
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshal for you.
    /// It also runs any configured interceptors.
    pub async fn read_rtcp(&self) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut internal = self.internal.lock().await;
        internal.read_rtcp().await
    }

    /// read_simulcast_rtcp is a convenience method that wraps ReadSimulcast and unmarshal for you
    pub async fn read_simulcast_rtcp(
        &self,
        rid: &str,
    ) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut internal = self.internal.lock().await;
        internal.read_simulcast_rtcp(rid).await
    }

    pub(crate) async fn have_received(&self) -> bool {
        let internal = self.internal.lock().await;
        internal.received_tx.is_none()
    }

    /// Stop irreversibly stops the RTPReceiver
    pub async fn stop(&self) -> Result<()> {
        let mut internal = self.internal.lock().await;

        let _d = {
            if internal.closed_tx.is_none() {
                return Ok(());
            }
            internal.closed_tx.take()
        };

        let mut errs = vec![];
        tokio::select! {
            _ = internal.received_rx.recv()=>{
                for t in &internal.tracks {
                    if let Some(rtcp_read_stream) = &t.rtcp_read_stream {
                        if let Err(err) = rtcp_read_stream.close().await {
                            errs.push(err);
                        }
                    }

                    if let Some(rtp_read_stream) = &t.rtp_read_stream {
                        if let Err(err) = rtp_read_stream.close().await {
                            errs.push(err);
                        }
                    }

                    if let Some(interceptor) = &internal.interceptor {
                        interceptor.unbind_remote_stream(&t.stream_info).await;
                    }
                }
            }
            else => {}
        }

        flatten_errs(errs)
    }

    /// read_rtp should only be called by a track, this only exists so we can keep state in one place
    pub(crate) async fn read_rtp(&self, b: &mut [u8], tid: &str) -> Result<(usize, Attributes)> {
        let mut internal = self.internal.lock().await;
        internal.read_rtp(b, tid).await
    }

    /// receive_for_rid is the sibling of Receive expect for RIDs instead of SSRCs
    /// It populates all the internal state for the given RID
    pub(crate) async fn receive_for_rid(
        &self,
        rid: &str,
        params: &RTPParameters,
        ssrc: SSRC,
    ) -> Result<Arc<TrackRemote>> {
        let mut internal = self.internal.lock().await;
        for t in &mut internal.tracks {
            if t.track.rid() == rid && !params.codecs.is_empty() {
                t.track.set_kind(self.kind);
                t.track.set_codec(params.codecs[0].clone()).await;
                t.track.set_params(params.clone()).await;
                t.track.set_ssrc(ssrc);
                t.stream_info = StreamInfo::new(
                    "".to_owned(),
                    ssrc,
                    params.codecs[0].payload_type,
                    params.codecs[0].capability.clone(),
                    &params.header_extensions,
                );

                let (rtp_read_stream, rtp_interceptor, rtcp_read_stream, rtcp_interceptor) =
                    RTPReceiver::streams_for_ssrc(&self.transport, ssrc, &t.stream_info).await?;

                t.rtp_read_stream = rtp_read_stream;
                t.rtp_interceptor = rtp_interceptor;
                t.rtcp_read_stream = rtcp_read_stream;
                t.rtcp_interceptor = rtcp_interceptor;

                return Ok(Arc::clone(&t.track));
            }
        }

        Err(Error::ErrRTPReceiverForSSRCTrackStreamNotFound.into())
    }

    async fn streams_for_ssrc(
        transport: &Arc<DTLSTransport>,
        ssrc: SSRC,
        _stream_info: &StreamInfo,
    ) -> Result<(
        Option<Arc<srtp::stream::Stream>>,
        Option<Arc<dyn RTPReader + Send + Sync>>,
        Option<Arc<srtp::stream::Stream>>,
        Option<Arc<dyn RTCPReader + Send + Sync>>,
    )> {
        let srtp_session = transport
            .get_srtp_session()
            .await
            .ok_or(Error::ErrDtlsTransportNotStarted)?;
        let rtp_read_stream = srtp_session.listen(ssrc).await?;

        /*TODO: rtp_interceptor := r.api.interceptor.bind_remote_stream(&streamInfo, interceptor.RTPReaderFunc(func(in []byte, a interceptor.Attributes) (n int, attributes interceptor.Attributes, err error) {
            n, err = rtp_read_stream.Read(in)
            return n, a, err
        }))*/
        let rtp_interceptor = None;

        let srtcp_session = transport
            .get_srtcp_session()
            .await
            .ok_or(Error::ErrDtlsTransportNotStarted)?;
        let rtcp_read_stream = srtcp_session.listen(ssrc).await?;

        /*TODO: rtcp_interceptor := r.api.interceptor.bind_rtcpreader(interceptor.RTPReaderFunc(func(in []byte, a interceptor.Attributes) (n int, attributes interceptor.Attributes, err error) {
            n, err = rtcp_read_stream.Read(in)
            return n, a, err
        }))*/
        let rtcp_interceptor = None;

        Ok((
            Some(Arc::new(rtp_read_stream)),
            rtp_interceptor,
            Some(Arc::new(rtcp_read_stream)),
            rtcp_interceptor,
        ))
    }
}
