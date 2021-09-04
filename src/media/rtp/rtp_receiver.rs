#[cfg(test)]
mod rtp_receiver_test;

use crate::api::media_engine::MediaEngine;
use crate::error::Error;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::interceptor::stream_info::{RTPHeaderExtension, StreamInfo};
use crate::media::interceptor::*;
use crate::media::rtp::rtp_codec::{
    codec_parameters_fuzzy_search, CodecMatch, RTPCodecCapability, RTPCodecParameters,
    RTPCodecType, RTPParameters,
};
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::rtp::{RTPCodingParameters, RTPReceiveParameters, SSRC};
use crate::media::track::track_remote::TrackRemote;
use crate::media::track::TrackStreams;
use crate::util::flatten_errs;
use crate::RECEIVE_MTU;

use crate::peer::sdp::TrackDetails;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

pub(crate) struct RTPReceiverInternal {
    pub(crate) kind: RTPCodecType,
    tracks: Vec<TrackStreams>,

    transport: Arc<DTLSTransport>,
    media_engine: Arc<MediaEngine>,
    interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,

    closed_tx: Option<mpsc::Sender<()>>,
    closed_rx: mpsc::Receiver<()>,
    received_tx: Option<mpsc::Sender<()>>,
    received_rx: mpsc::Receiver<()>,

    transceiver_codecs: Option<Arc<Mutex<Vec<RTPCodecParameters>>>>,
}

impl RTPReceiverInternal {
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

    pub(crate) async fn read_rtp(
        &mut self,
        b: &mut [u8],
        tid: &str,
    ) -> Result<(usize, Attributes)> {
        let _ = self.received_rx.recv().await;

        for t in &self.tracks {
            if t.track.id().await == tid {
                if let Some(ri) = &t.rtp_interceptor {
                    let a = Attributes::new();
                    return ri.read(b, &a).await;
                }
            }
        }

        Err(Error::ErrRTPReceiverWithSSRCTrackStreamNotFound.into())
    }

    async fn get_parameters(&self) -> RTPParameters {
        let mut parameters = self
            .media_engine
            .get_rtp_parameters_by_kind(self.kind, &[RTPTransceiverDirection::Recvonly])
            .await;

        if let Some(codecs) = &self.transceiver_codecs {
            let c = codecs.lock().await;
            parameters.codecs =
                RTPReceiverInternal::get_codecs(&*c, self.kind, &self.media_engine).await;
        }

        parameters
    }

    pub(crate) async fn get_codecs(
        codecs: &[RTPCodecParameters],
        kind: RTPCodecType,
        media_engine: &Arc<MediaEngine>,
    ) -> Vec<RTPCodecParameters> {
        let media_engine_codecs = media_engine.get_codecs_by_kind(kind).await;
        if codecs.is_empty() {
            return media_engine_codecs;
        }
        let mut filtered_codecs = vec![];
        for codec in &*codecs {
            let (c, match_type) = codec_parameters_fuzzy_search(codec, &media_engine_codecs);
            if match_type != CodecMatch::None {
                filtered_codecs.push(c);
            }
        }

        filtered_codecs
    }
}

/// RTPReceiver allows an application to inspect the receipt of a TrackRemote
pub struct RTPReceiver {
    kind: RTPCodecType,
    transport: Arc<DTLSTransport>,
    pub(crate) internal: Arc<Mutex<RTPReceiverInternal>>,
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
            internal: Arc::new(Mutex::new(RTPReceiverInternal {
                kind,

                tracks: vec![],
                transport,
                media_engine,
                interceptor,

                closed_tx: Some(closed_tx),
                closed_rx,
                received_tx: Some(received_tx),
                received_rx,

                transceiver_codecs: None,
            })),
        }
    }

    pub fn kind(&self) -> RTPCodecType {
        self.kind
    }

    pub(crate) async fn set_transceiver_codecs(
        &self,
        codecs: Option<Arc<Mutex<Vec<RTPCodecParameters>>>>,
    ) {
        let mut internal = self.internal.lock().await;
        internal.transceiver_codecs = codecs;
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

    /// SetRTPParameters applies provided RTPParameters the RTPReceiver's tracks.
    /// This method is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    /// The amount of provided codecs must match the number of tracks on the receiver.
    pub async fn set_rtp_parameters(&self, params: RTPParameters) {
        let mut header_extensions = vec![];
        for h in &params.header_extensions {
            header_extensions.push(RTPHeaderExtension {
                id: h.id,
                uri: h.uri.clone(),
            });
        }

        let mut internal = self.internal.lock().await;
        for (idx, codec) in params.codecs.iter().enumerate() {
            let t = &mut internal.tracks[idx];
            t.stream_info.rtp_header_extensions = header_extensions.clone();

            let current_track = &t.track;
            current_track.set_codec(codec.clone()).await;
            current_track.set_params(params.clone()).await;
        }
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
        let receiver = Arc::clone(&self.internal);
        let mut internal = self.internal.lock().await;
        if internal.received_tx.is_none() {
            return Err(Error::ErrRTPReceiverReceiveAlreadyCalled.into());
        }
        let _d = internal.received_tx.take(); // defer drop(received_tx)

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
                        receiver,
                        Arc::clone(&internal.media_engine),
                        internal.interceptor.clone(),
                    )),
                    stream_info,
                    rtp_read_stream,
                    rtp_interceptor,
                    rtcp_read_stream,
                    rtcp_interceptor,
                };

                internal.tracks.push(t);
            }
        } else {
            for encoding in &parameters.encodings {
                let t = TrackStreams {
                    track: Arc::new(TrackRemote::new(
                        self.kind,
                        0,
                        encoding.rid.clone(),
                        Arc::clone(&receiver),
                        Arc::clone(&internal.media_engine),
                        internal.interceptor.clone(),
                    )),
                    ..Default::default()
                };

                internal.tracks.push(t);
            }
        }

        Ok(())
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

    pub(crate) async fn start(&self, incoming: &TrackDetails) -> bool {
        let mut encodings = vec![];
        if incoming.ssrc != 0 {
            encodings.push(RTPCodingParameters {
                ssrc: incoming.ssrc,
                ..Default::default()
            });
        }
        for rid in &incoming.rids {
            encodings.push(RTPCodingParameters {
                rid: rid.to_owned(),
                ..Default::default()
            });
        }

        if let Err(err) = self.receive(&RTPReceiveParameters { encodings }).await {
            log::warn!("RTPReceiver Receive failed {}", err);
            return false;
        }

        // set track id and label early so they can be set as new track information
        // is received from the SDP.
        for track_remote in &self.tracks().await {
            track_remote.set_id(incoming.id.clone()).await;
            track_remote.set_stream_id(incoming.stream_id.clone()).await;
        }

        // We can't block and wait for a single SSRC
        incoming.ssrc != 0

        /*
        let media_engine = Arc::clone(&self.media_engine);
        tokio::spawn(async move {
            if let Some(track) = receiver.track().await {
                if let Err(err) = track.determine_payload_type().await {
                    log::warn!(
                        "Could not determine PayloadType for SSRC {} with err {}",
                        track.ssrc(),
                        err
                    );
                    return;
                }

                let params = match media_engine
                    .get_rtp_parameters_by_payload_type(track.payload_type())
                    .await
                {
                    Ok(params) => params,
                    Err(err) => {
                        log::warn!(
                            "no codec could be found for payloadType {} with err {}",
                            track.payload_type(),
                            err,
                        );
                        return;
                    }
                };

                track.set_kind(receiver.kind());
                track.set_codec(params.codecs[0].clone()).await;
                track.set_params(params).await;

                //TODO:self.do_track(receiver.track().await, Some(Arc::clone(&receiver))).await;
            }
        });*/
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
        if internal.received_tx.is_none() {
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
