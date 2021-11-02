#[cfg(test)]
mod rtp_receiver_test;

use crate::api::media_engine::MediaEngine;
use crate::dtls_transport::RTCDtlsTransport;
use crate::error::{flatten_errs, Error, Result};
use crate::peer_connection::sdp::TrackDetails;
use crate::rtp_transceiver::rtp_codec::{
    codec_parameters_fuzzy_search, CodecMatch, RTCRtpCodecCapability, RTCRtpCodecParameters,
    RTCRtpParameters, RTPCodecType,
};
use crate::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use crate::rtp_transceiver::{
    create_stream_info, RTCRtpDecodingParameters, RTCRtpReceiveParameters, SSRC,
};
use crate::track::track_remote::TrackRemote;
use crate::track::TrackStreams;

use interceptor::stream_info::{RTPHeaderExtension, StreamInfo};
use interceptor::{Attributes, Interceptor, RTCPReader, RTPReader};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify};

pub struct RTPReceiverInternal {
    pub(crate) kind: RTPCodecType,
    tracks: Mutex<Vec<TrackStreams>>,
    closed_rx: Arc<Notify>,
    received_rx: Mutex<mpsc::Receiver<()>>,

    transceiver_codecs: Mutex<Option<Arc<Mutex<Vec<RTCRtpCodecParameters>>>>>,

    transport: Arc<RTCDtlsTransport>,
    media_engine: Arc<MediaEngine>,
    interceptor: Arc<dyn Interceptor + Send + Sync>,
}

impl RTPReceiverInternal {
    /// read reads incoming RTCP for this RTPReceiver
    async fn read(&self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        let mut received_rx = self.received_rx.lock().await;

        tokio::select! {
            _ = received_rx.recv() =>{
                let tracks = self.tracks.lock().await;
                if let Some(t) = tracks.first(){
                    if let Some(rtcp_interceptor) = &t.rtcp_interceptor{
                        let a = Attributes::new();
                        tokio::select! {
                            _ = self.closed_rx.notified() => {
                                Err(Error::ErrClosedPipe)
                            }
                            result = rtcp_interceptor.read(b, &a) => {
                                Ok(result?)
                            }
                        }
                    }else{
                        Err(Error::ErrInterceptorNotBind)
                    }
                }else{
                    Err(Error::ErrExistingTrack)
                }
            }
            _ = self.closed_rx.notified() => {
                Err(Error::ErrClosedPipe)
            }
        }
    }

    /// read_simulcast reads incoming RTCP for this RTPReceiver for given rid
    async fn read_simulcast(&self, b: &mut [u8], rid: &str) -> Result<(usize, Attributes)> {
        let mut received_rx = self.received_rx.lock().await;

        tokio::select! {
            _ = received_rx.recv() =>{
                let tracks = self.tracks.lock().await;
                for t in &*tracks{
                    if t.track.rid() == rid {
                       if let Some(rtcp_interceptor) = &t.rtcp_interceptor{
                            let a = Attributes::new();
                            tokio::select! {
                                _ = self.closed_rx.notified() => {
                                    return Err(Error::ErrClosedPipe);
                                }
                                result = rtcp_interceptor.read(b, &a) => {
                                    return Ok(result?);
                                }
                            }
                        }else{
                            return Err(Error::ErrInterceptorNotBind);
                        }
                    }
                }
                Err(Error::ErrRTPReceiverForRIDTrackStreamNotFound)
            }
            _ = self.closed_rx.notified() => {
                Err(Error::ErrClosedPipe)
            }
        }
    }

    /// read_rtcp is a convenience method that wraps Read and unmarshal for you.
    /// It also runs any configured interceptors.
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

    /// read_simulcast_rtcp is a convenience method that wraps ReadSimulcast and unmarshal for you
    async fn read_simulcast_rtcp(
        &self,
        rid: &str,
        receive_mtu: usize,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        let mut b = vec![0u8; receive_mtu];
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

        if let Some(rtp_interceptor) = rtp_interceptor {
            let a = Attributes::new();
            tokio::select! {
                _ = self.closed_rx.notified() => {
                    Err(Error::ErrClosedPipe)
                }
                result = rtp_interceptor.read(b, &a) => {
                    Ok(result?)
                }
            }
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
    receive_mtu: usize,
    kind: RTPCodecType,
    transport: Arc<RTCDtlsTransport>,
    closed_tx: Arc<Notify>,
    received_tx: Mutex<Option<mpsc::Sender<()>>>,

    pub internal: Arc<RTPReceiverInternal>,
}

impl RTCRtpReceiver {
    pub fn new(
        receive_mtu: usize,
        kind: RTPCodecType,
        transport: Arc<RTCDtlsTransport>,
        media_engine: Arc<MediaEngine>,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
    ) -> Self {
        let closed_tx = Arc::new(Notify::new());
        let closed_rx = closed_tx.clone();
        let (received_tx, received_rx) = mpsc::channel(1);

        RTCRtpReceiver {
            receive_mtu,
            kind,
            transport: Arc::clone(&transport),
            closed_tx,
            received_tx: Mutex::new(Some(received_tx)),

            internal: Arc::new(RTPReceiverInternal {
                kind,

                tracks: Mutex::new(vec![]),
                transport,
                media_engine,
                interceptor,

                closed_rx,
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
            if let Some(stream_info) = &mut t.stream_info {
                stream_info.rtp_header_extensions = header_extensions.clone();
            }

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
        let receiver = Arc::downgrade(&self.internal);

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

        let codec = if let Some(codec) = global_params.codecs.first() {
            codec.capability.clone()
        } else {
            RTCRtpCodecCapability::default()
        };

        let mut tracks = vec![];
        for encoding in &parameters.encodings {
            let (stream_info, rtp_read_stream, rtp_interceptor, rtcp_read_stream, rtcp_interceptor) =
                if encoding.ssrc != 0 {
                    let stream_info = create_stream_info(
                        "".to_owned(),
                        encoding.ssrc,
                        0,
                        codec.clone(),
                        &global_params.header_extensions,
                    );
                    let (rtp_read_stream, rtp_interceptor, rtcp_read_stream, rtcp_interceptor) =
                        self.transport
                            .streams_for_ssrc(encoding.ssrc, &stream_info, &interceptor)
                            .await?;

                    (
                        Some(stream_info),
                        rtp_read_stream,
                        rtp_interceptor,
                        rtcp_read_stream,
                        rtcp_interceptor,
                    )
                } else {
                    (None, None, None, None, None)
                };

            let t = TrackStreams {
                track: Arc::new(TrackRemote::new(
                    self.receive_mtu,
                    self.kind,
                    encoding.ssrc,
                    encoding.rid.clone(),
                    receiver.clone(),
                    Arc::clone(&media_engine),
                    Arc::clone(&interceptor),
                )),
                stream_info,
                rtp_read_stream,
                rtp_interceptor,
                rtcp_read_stream,
                rtcp_interceptor,

                repair_stream_info: None,
                repair_rtp_read_stream: None,
                repair_rtp_interceptor: None,
                repair_rtcp_read_stream: None,
                repair_rtcp_interceptor: None,
            };

            tracks.push(t);

            let rtx_ssrc = encoding.rtx.ssrc;
            if rtx_ssrc != 0 {
                let stream_info = create_stream_info(
                    "".to_owned(),
                    rtx_ssrc,
                    0,
                    codec.clone(),
                    &global_params.header_extensions,
                );
                let (rtp_read_stream, rtp_interceptor, rtcp_read_stream, rtcp_interceptor) = self
                    .transport
                    .streams_for_ssrc(rtx_ssrc, &stream_info, &interceptor)
                    .await?;

                self.receive_for_rtx(
                    rtx_ssrc,
                    "".to_owned(),
                    stream_info,
                    rtp_read_stream,
                    rtp_interceptor,
                    rtcp_read_stream,
                    rtcp_interceptor,
                )
                .await?;
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
    pub async fn read_rtcp(
        &self,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        self.internal.read_rtcp(self.receive_mtu).await
    }

    /// read_simulcast_rtcp is a convenience method that wraps ReadSimulcast and unmarshal for you
    pub async fn read_simulcast_rtcp(
        &self,
        rid: &str,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        self.internal
            .read_simulcast_rtcp(rid, self.receive_mtu)
            .await
    }

    pub(crate) async fn have_received(&self) -> bool {
        let received_tx = self.received_tx.lock().await;
        received_tx.is_none()
    }

    pub(crate) async fn start(&self, incoming: &TrackDetails) {
        let mut encodingSize = incoming.ssrcs.len();
        if incoming.rids.len() >= encodingSize {
            encodingSize = incoming.rids.len();
        };

        let mut encodings = vec![RTCRtpDecodingParameters::default(); encodingSize];
        for i in 0..encodings.len() {
            if incoming.rids.len() > i {
                encodings[i].rid = incoming.rids[i].clone();
            }
            if incoming.ssrcs.len() > i {
                encodings[i].ssrc = incoming.ssrcs[i];
            }

            encodings[i].rtx.ssrc = incoming.repair_ssrc;
        }

        if let Err(err) = self.receive(&RTCRtpReceiveParameters { encodings }).await {
            log::warn!("RTPReceiver Receive failed {}", err);
            return;
        }

        // set track id and label early so they can be set as new track information
        // is received from the SDP.
        for track_remote in &self.tracks().await {
            track_remote.set_id(incoming.id.clone()).await;
            track_remote.set_stream_id(incoming.stream_id.clone()).await;
        }
    }

    /// Stop irreversibly stops the RTPReceiver
    pub async fn stop(&self) -> Result<()> {
        self.closed_tx.notify_waiters();

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

                if let Some(repair_rtcp_read_stream) = &t.repair_rtcp_read_stream {
                    if let Err(err) = repair_rtcp_read_stream.close().await {
                        errs.push(err);
                    }
                }

                if let Some(repair_rtp_read_stream) = &t.repair_rtp_read_stream {
                    if let Err(err) = repair_rtp_read_stream.close().await {
                        errs.push(err);
                    }
                }

                if let Some(stream_info) = &t.stream_info {
                    self.internal
                        .interceptor
                        .unbind_remote_stream(stream_info)
                        .await;
                }

                if let Some(repair_stream_info) = &t.repair_stream_info {
                    self.internal
                        .interceptor
                        .unbind_remote_stream(repair_stream_info)
                        .await;
                }
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
        rid: String,
        params: RTCRtpParameters,
        stream_info: StreamInfo,
        rtp_read_stream: Option<Arc<srtp::stream::Stream>>,
        rtp_interceptor: Option<Arc<dyn RTPReader + Send + Sync>>,
        rtcp_read_stream: Option<Arc<srtp::stream::Stream>>,
        rtcp_interceptor: Option<Arc<dyn RTCPReader + Send + Sync>>,
    ) -> Result<Arc<TrackRemote>> {
        let mut tracks = self.internal.tracks.lock().await;
        for t in &mut *tracks {
            if t.track.rid() == rid && !params.codecs.is_empty() {
                t.track.set_kind(self.kind);
                t.track.set_codec(params.codecs[0].clone()).await;
                t.track.set_params(params.clone()).await;
                t.track.set_ssrc(stream_info.ssrc);
                t.stream_info = Some(stream_info);
                t.rtp_read_stream = rtp_read_stream;
                t.rtp_interceptor = rtp_interceptor;
                t.rtcp_read_stream = rtcp_read_stream;
                t.rtcp_interceptor = rtcp_interceptor;

                return Ok(Arc::clone(&t.track));
            }
        }

        Err(Error::ErrRTPReceiverForRIDTrackStreamNotFound)
    }

    /// receiveForRtx starts a routine that processes the repair stream
    /// These packets aren't exposed to the user yet, but we need to process them for
    /// TWCC
    pub(crate) async fn receive_for_rtx(
        &self,
        ssrc: SSRC,
        rsid: String,
        stream_info: StreamInfo,
        rtp_read_stream: Option<Arc<srtp::stream::Stream>>,
        rtp_interceptor: Option<Arc<dyn RTPReader + Send + Sync>>,
        rtcp_read_stream: Option<Arc<srtp::stream::Stream>>,
        rtcp_interceptor: Option<Arc<dyn RTCPReader + Send + Sync>>,
    ) -> Result<()> {
        let mut tracks = self.internal.tracks.lock().await;
        let l = tracks.len();
        for t in &mut *tracks {
            if (ssrc != 0 && l == 1) || t.track.rid() == rsid {
                t.repair_stream_info = Some(stream_info);
                t.repair_rtp_read_stream = rtp_read_stream;
                t.repair_rtp_interceptor = rtp_interceptor;
                t.repair_rtcp_read_stream = rtcp_read_stream;
                t.repair_rtcp_interceptor = rtcp_interceptor;

                let receive_mtu = self.receive_mtu;
                let track = t.clone();
                tokio::spawn(async move {
                    let a = Attributes::new();
                    let mut b = vec![0u8; receive_mtu];
                    loop {
                        if let Some(repair_rtp_interceptor) = &track.repair_rtp_interceptor {
                            //TODO: cancel repair_rtp_interceptor.read gracefully
                            if let Err(err) = repair_rtp_interceptor.read(&mut b, &a).await {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                });

                return Ok(());
            }
        }

        Err(Error::ErrRTPReceiverForRIDTrackStreamNotFound)
    }
}
