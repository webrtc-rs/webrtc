#[cfg(test)]
mod rtp_receiver_test;

use crate::api::media_engine::MediaEngine;
use crate::dtls_transport::RTCDtlsTransport;
use crate::error::{Error, Result};
use crate::peer_connection::sdp::TrackDetails;
use crate::rtp_transceiver::rtp_codec::{
    codec_parameters_fuzzy_search, CodecMatch, RTCRtpCodecCapability, RTCRtpCodecParameters,
    RTCRtpParameters, RTPCodecType,
};
use crate::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use crate::rtp_transceiver::{
    create_stream_info, RTCRtpCodingParameters, RTCRtpReceiveParameters, SSRC,
};
use crate::track::track_remote::TrackRemote;
use crate::track::TrackStreams;
use crate::util::flatten_errs;
use crate::RECEIVE_MTU;

use interceptor::stream_info::{RTPHeaderExtension, StreamInfo};
use interceptor::{Attributes, Interceptor, RTCPReader, RTPReader};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

pub(crate) struct RTPReceiverInternal {
    pub(crate) kind: RTPCodecType,
    tracks: Mutex<Vec<TrackStreams>>,
    closed_rx: Mutex<mpsc::Receiver<()>>,
    received_rx: Mutex<mpsc::Receiver<()>>,

    transceiver_codecs: Mutex<Option<Arc<Mutex<Vec<RTCRtpCodecParameters>>>>>,

    transport: Arc<RTCDtlsTransport>,
    media_engine: Arc<MediaEngine>,
    interceptor: Arc<dyn Interceptor + Send + Sync>,
}

impl RTPReceiverInternal {
    /// read reads incoming RTCP for this RTPReceiver
    async fn read(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        let (mut received_rx, mut closed_rx) =
            (self.received_rx.lock().await, self.closed_rx.lock().await);

        tokio::select! {
            _ = received_rx.recv() =>{
                let tracks = self.tracks.lock().await;
                if let Some(t) = tracks.first(){
                    if let Some(rtcp_interceptor) = &t.rtcp_interceptor{
                        let a = Attributes::new();
                        Ok(rtcp_interceptor.read(b, &a).await?)
                    }else{
                        Err(Error::ErrInterceptorNotBind)
                    }
                }else{
                    Err(Error::ErrExistingTrack)
                }
            }
            _ = closed_rx.recv() => {
                Err(Error::ErrClosedPipe)
            }
        }
    }

    /// read_simulcast reads incoming RTCP for this RTPReceiver for given rid
    async fn read_simulcast(&self, b: &mut [u8], rid: &str) -> Result<(usize, Attributes)> {
        let (mut received_rx, mut closed_rx) =
            (self.received_rx.lock().await, self.closed_rx.lock().await);

        tokio::select! {
            _ = received_rx.recv() =>{
                let tracks = self.tracks.lock().await;
                for t in &*tracks{
                    if t.track.rid() == rid {
                       if let Some(rtcp_interceptor) = &t.rtcp_interceptor{
                            let a = Attributes::new();
                            return Ok(rtcp_interceptor.read(b, &a).await?);
                        }else{
                            return Err(Error::ErrInterceptorNotBind);
                        }
                    }
                }
                Err(Error::ErrRTPReceiverForRIDTrackStreamNotFound)
            }
            _ = closed_rx.recv() => {
                Err(Error::ErrClosedPipe)
            }
        }
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshal for you.
    /// It also runs any configured interceptors.
    async fn read_rtcp(&self) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, attributes) = self.read(&mut b).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }

    /// read_simulcast_rtcp is a convenience method that wraps ReadSimulcast and unmarshal for you
    async fn read_simulcast_rtcp(
        &self,
        rid: &str,
    ) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, attributes) = self.read_simulcast(&mut b, rid).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }

    pub(crate) async fn read_rtp(&self, b: &mut [u8], tid: usize) -> Result<(usize, Attributes)> {
        {
            let mut received_rx = self.received_rx.lock().await;
            let _ = received_rx.recv().await;
        }

        //log::debug!("read_rtp enter tracks tid {}", tid);
        let mut rtp_interceptor = None;
        {
            let tracks = self.tracks.lock().await;
            for t in &*tracks {
                if t.track.tid() == tid {
                    rtp_interceptor = t.rtp_interceptor.clone();
                    break;
                }
            }
        };
        /*log::debug!(
            "read_rtp exit tracks with rtp_interceptor {} with tid {}",
            rtp_interceptor.is_some(),
            tid,
        );*/

        if let Some(ri) = rtp_interceptor {
            let a = Attributes::new();
            Ok(ri.read(b, &a).await?)
        } else {
            //log::debug!("read_rtp exit tracks with ErrRTPReceiverWithSSRCTrackStreamNotFound");
            Err(Error::ErrRTPReceiverWithSSRCTrackStreamNotFound)
        }
    }

    async fn get_parameters(&self) -> RTCRtpParameters {
        let mut parameters = self
            .media_engine
            .get_rtp_parameters_by_kind(self.kind, &[RTCRtpTransceiverDirection::Recvonly])
            .await;

        let transceiver_codecs = self.transceiver_codecs.lock().await;
        if let Some(codecs) = &*transceiver_codecs {
            let c = codecs.lock().await;
            parameters.codecs =
                RTPReceiverInternal::get_codecs(&*c, self.kind, &self.media_engine).await;
        }

        parameters
    }

    pub(crate) async fn get_codecs(
        codecs: &[RTCRtpCodecParameters],
        kind: RTPCodecType,
        media_engine: &Arc<MediaEngine>,
    ) -> Vec<RTCRtpCodecParameters> {
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
pub struct RTCRtpReceiver {
    kind: RTPCodecType,
    transport: Arc<RTCDtlsTransport>,
    closed_tx: Mutex<Option<mpsc::Sender<()>>>,
    received_tx: Mutex<Option<mpsc::Sender<()>>>,

    pub(crate) internal: Arc<RTPReceiverInternal>,
}

impl RTCRtpReceiver {
    pub fn new(
        kind: RTPCodecType,
        transport: Arc<RTCDtlsTransport>,
        media_engine: Arc<MediaEngine>,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
    ) -> Self {
        let (closed_tx, closed_rx) = mpsc::channel(1);
        let (received_tx, received_rx) = mpsc::channel(1);

        RTCRtpReceiver {
            kind,
            transport: Arc::clone(&transport),
            closed_tx: Mutex::new(Some(closed_tx)),
            received_tx: Mutex::new(Some(received_tx)),

            internal: Arc::new(RTPReceiverInternal {
                kind,

                tracks: Mutex::new(vec![]),
                transport,
                media_engine,
                interceptor,

                closed_rx: Mutex::new(closed_rx),
                received_rx: Mutex::new(received_rx),

                transceiver_codecs: Mutex::new(None),
            }),
        }
    }

    pub fn kind(&self) -> RTPCodecType {
        self.kind
    }

    pub(crate) async fn set_transceiver_codecs(
        &self,
        codecs: Option<Arc<Mutex<Vec<RTCRtpCodecParameters>>>>,
    ) {
        let mut transceiver_codecs = self.internal.transceiver_codecs.lock().await;
        *transceiver_codecs = codecs;
    }

    /// transport returns the currently-configured *DTLSTransport or nil
    /// if one has not yet been configured
    pub fn transport(&self) -> Arc<RTCDtlsTransport> {
        Arc::clone(&self.transport)
    }

    /// get_parameters describes the current configuration for the encoding and
    /// transmission of media on the receiver's track.
    pub async fn get_parameters(&self) -> RTCRtpParameters {
        self.internal.get_parameters().await
    }

    /// SetRTPParameters applies provided RTPParameters the RTPReceiver's tracks.
    /// This method is part of the ORTC API. It is not
    /// meant to be used together with the basic WebRTC API.
    /// The amount of provided codecs must match the number of tracks on the receiver.
    pub async fn set_rtp_parameters(&self, params: RTCRtpParameters) {
        let mut header_extensions = vec![];
        for h in &params.header_extensions {
            header_extensions.push(RTPHeaderExtension {
                id: h.id,
                uri: h.uri.clone(),
            });
        }

        let mut tracks = self.internal.tracks.lock().await;
        for (idx, codec) in params.codecs.iter().enumerate() {
            let t = &mut tracks[idx];
            t.stream_info.rtp_header_extensions = header_extensions.clone();

            let current_track = &t.track;
            current_track.set_codec(codec.clone()).await;
            current_track.set_params(params.clone()).await;
        }
    }

    /// track returns the RtpTransceiver TrackRemote
    pub async fn track(&self) -> Option<Arc<TrackRemote>> {
        let tracks = self.internal.tracks.lock().await;
        if tracks.len() != 1 {
            None
        } else {
            tracks.first().map(|t| Arc::clone(&t.track))
        }
    }

    /// tracks returns the RtpTransceiver tracks
    /// A RTPReceiver to support Simulcast may now have multiple tracks
    pub async fn tracks(&self) -> Vec<Arc<TrackRemote>> {
        let tracks = self.internal.tracks.lock().await;
        tracks.iter().map(|t| Arc::clone(&t.track)).collect()
    }

    /// receive initialize the track and starts all the transports
    pub async fn receive(&self, parameters: &RTCRtpReceiveParameters) -> Result<()> {
        let receiver = Arc::clone(&self.internal);

        let _d = {
            let mut received_tx = self.received_tx.lock().await;
            if received_tx.is_none() {
                return Err(Error::ErrRTPReceiverReceiveAlreadyCalled);
            }
            received_tx.take()
        };

        let (global_params, interceptor, media_engine) = {
            (
                self.internal.get_parameters().await,
                Arc::clone(&self.internal.interceptor),
                Arc::clone(&self.internal.media_engine),
            )
        };

        let mut tracks = vec![];
        if parameters.encodings.len() == 1 && parameters.encodings[0].ssrc != 0 {
            if let Some(encoding) = parameters.encodings.first() {
                let codec = if let Some(codec) = global_params.codecs.first() {
                    codec.capability.clone()
                } else {
                    RTCRtpCodecCapability::default()
                };

                let stream_info = create_stream_info(
                    "".to_owned(),
                    encoding.ssrc,
                    0,
                    codec,
                    &global_params.header_extensions,
                );
                let (rtp_read_stream, rtp_interceptor, rtcp_read_stream, rtcp_interceptor) =
                    RTCRtpReceiver::streams_for_ssrc(
                        &self.transport,
                        encoding.ssrc,
                        &stream_info,
                        &interceptor,
                    )
                    .await?;

                let t = TrackStreams {
                    track: Arc::new(TrackRemote::new(
                        self.kind,
                        encoding.ssrc,
                        "".to_owned(),
                        receiver,
                        Arc::clone(&media_engine),
                        Arc::clone(&interceptor),
                    )),
                    stream_info,
                    rtp_read_stream,
                    rtp_interceptor,
                    rtcp_read_stream,
                    rtcp_interceptor,
                };

                tracks.push(t);
            }
        } else {
            for encoding in &parameters.encodings {
                let t = TrackStreams {
                    track: Arc::new(TrackRemote::new(
                        self.kind,
                        0,
                        encoding.rid.clone(),
                        Arc::clone(&receiver),
                        Arc::clone(&media_engine),
                        Arc::clone(&interceptor),
                    )),
                    stream_info: Default::default(),
                    rtp_read_stream: None,
                    rtp_interceptor: None,
                    rtcp_read_stream: None,
                    rtcp_interceptor: None,
                };

                tracks.push(t);
            }
        }

        {
            let mut internal_tracks = self.internal.tracks.lock().await;
            internal_tracks.extend(tracks);
        };

        Ok(())
    }

    /// read reads incoming RTCP for this RTPReceiver
    pub async fn read(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        self.internal.read(b).await
    }

    /// read_simulcast reads incoming RTCP for this RTPReceiver for given rid
    pub async fn read_simulcast(&self, b: &mut [u8], rid: &str) -> Result<(usize, Attributes)> {
        self.internal.read_simulcast(b, rid).await
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshal for you.
    /// It also runs any configured interceptors.
    pub async fn read_rtcp(&self) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        self.internal.read_rtcp().await
    }

    /// read_simulcast_rtcp is a convenience method that wraps ReadSimulcast and unmarshal for you
    pub async fn read_simulcast_rtcp(
        &self,
        rid: &str,
    ) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        self.internal.read_simulcast_rtcp(rid).await
    }

    pub(crate) async fn have_received(&self) -> bool {
        let received_tx = self.received_tx.lock().await;
        received_tx.is_none()
    }

    pub(crate) async fn start(&self, incoming: &TrackDetails) -> bool {
        let mut encodings = vec![];
        if incoming.ssrc != 0 {
            encodings.push(RTCRtpCodingParameters {
                ssrc: incoming.ssrc,
                ..Default::default()
            });
        }
        for rid in &incoming.rids {
            encodings.push(RTCRtpCodingParameters {
                rid: rid.to_owned(),
                ..Default::default()
            });
        }

        if let Err(err) = self.receive(&RTCRtpReceiveParameters { encodings }).await {
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
    }

    /// Stop irreversibly stops the RTPReceiver
    pub async fn stop(&self) -> Result<()> {
        let _d = {
            let mut closed_tx = self.closed_tx.lock().await;
            if closed_tx.is_none() {
                return Ok(());
            }
            closed_tx.take()
        };

        let received_tx_is_none = {
            let received_tx = self.received_tx.lock().await;
            received_tx.is_none()
        };

        let mut errs = vec![];
        if received_tx_is_none {
            let tracks = self.internal.tracks.lock().await;
            for t in &*tracks {
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

                self.internal
                    .interceptor
                    .unbind_remote_stream(&t.stream_info)
                    .await;
            }
        }

        flatten_errs(errs)
    }

    /// read_rtp should only be called by a track, this only exists so we can keep state in one place
    pub(crate) async fn read_rtp(&self, b: &mut [u8], tid: usize) -> Result<(usize, Attributes)> {
        self.internal.read_rtp(b, tid).await
    }

    /// receive_for_rid is the sibling of Receive expect for RIDs instead of SSRCs
    /// It populates all the internal state for the given RID
    pub(crate) async fn receive_for_rid(
        &self,
        rid: &str,
        params: &RTCRtpParameters,
        ssrc: SSRC,
    ) -> Result<Arc<TrackRemote>> {
        let interceptor = Arc::clone(&self.internal.interceptor);
        //log::debug!("receive_for_rid enter tracks");
        {
            let mut tracks = self.internal.tracks.lock().await;
            for t in &mut *tracks {
                if t.track.rid() == rid && !params.codecs.is_empty() {
                    t.track.set_kind(self.kind);
                    t.track.set_codec(params.codecs[0].clone()).await;
                    t.track.set_params(params.clone()).await;
                    t.track.set_ssrc(ssrc);
                    t.stream_info = create_stream_info(
                        "".to_owned(),
                        ssrc,
                        params.codecs[0].payload_type,
                        params.codecs[0].capability.clone(),
                        &params.header_extensions,
                    );

                    let (rtp_read_stream, rtp_interceptor, rtcp_read_stream, rtcp_interceptor) =
                        RTCRtpReceiver::streams_for_ssrc(
                            &self.transport,
                            ssrc,
                            &t.stream_info,
                            &interceptor,
                        )
                        .await?;

                    t.rtp_read_stream = rtp_read_stream;
                    t.rtp_interceptor = rtp_interceptor;
                    t.rtcp_read_stream = rtcp_read_stream;
                    t.rtcp_interceptor = rtcp_interceptor;

                    //log::debug!("receive_for_rid exit tracks 1");
                    return Ok(Arc::clone(&t.track));
                }
            }
        }

        //log::debug!("receive_for_rid exit tracks 2");
        Err(Error::ErrRTPReceiverForSSRCTrackStreamNotFound)
    }

    async fn streams_for_ssrc(
        transport: &Arc<RTCDtlsTransport>,
        ssrc: SSRC,
        stream_info: &StreamInfo,
        interceptor: &Arc<dyn Interceptor + Send + Sync>,
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
        //log::debug!("streams_for_ssrc: srtp_session.listen ssrc={}", ssrc);
        let rtp_read_stream = srtp_session.open(ssrc).await;
        let rtp_stream_reader = Arc::clone(&rtp_read_stream) as Arc<dyn RTPReader + Send + Sync>;
        let rtp_interceptor = interceptor
            .bind_remote_stream(stream_info, rtp_stream_reader)
            .await;

        let srtcp_session = transport
            .get_srtcp_session()
            .await
            .ok_or(Error::ErrDtlsTransportNotStarted)?;
        //log::debug!("streams_for_ssrc: srtcp_session.listen ssrc={}", ssrc);
        let rtcp_read_stream = srtcp_session.open(ssrc).await;
        let rtcp_stream_reader = Arc::clone(&rtcp_read_stream) as Arc<dyn RTCPReader + Send + Sync>;
        let rtcp_interceptor = interceptor.bind_rtcp_reader(rtcp_stream_reader).await;

        Ok((
            Some(rtp_read_stream),
            Some(rtp_interceptor),
            Some(rtcp_read_stream),
            Some(rtcp_interceptor),
        ))
    }
}
