use crate::api::media_engine::MediaEngine;
use crate::error::Error;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::interceptor::stream_info::StreamInfo;
use crate::media::interceptor::{Attributes, Interceptor, RTCPReader, RTPReader};
use crate::media::rtp::rtp_codec::{RTPCodecCapability, RTPCodecType, RTPParameters};
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::rtp::{RTPReceiveParameters, SSRC};
use crate::media::track::track_remote::TrackRemote;
use crate::media::track::TrackStreams;

use crate::RECEIVE_MTU;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;

/// RTPReceiver allows an application to inspect the receipt of a TrackRemote
pub struct RTPReceiver {
    pub(crate) kind: RTPCodecType,
    pub(crate) transport: Arc<DTLSTransport>,

    pub(crate) tracks: Vec<TrackStreams>,

    pub(crate) closed_tx: Option<mpsc::Sender<()>>,
    pub(crate) closed_rx: mpsc::Receiver<()>,
    pub(crate) received_tx: Option<mpsc::Sender<()>>,
    pub(crate) received_rx: mpsc::Receiver<()>,

    pub(crate) media_engine: Arc<MediaEngine>,
    pub(crate) interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,
}

impl RTPReceiver {
    /// transport returns the currently-configured *DTLSTransport or nil
    /// if one has not yet been configured
    pub fn transport(&self) -> Arc<DTLSTransport> {
        Arc::clone(&self.transport)
    }

    /// get_parameters describes the current configuration for the encoding and
    /// transmission of media on the receiver's track.
    pub async fn get_parameters(&self) -> RTPParameters {
        self.media_engine
            .get_rtp_parameters_by_kind(self.kind, &[RTPTransceiverDirection::Recvonly])
            .await
    }

    /// track returns the RtpTransceiver TrackRemote
    pub fn track(&self) -> Option<Arc<TrackRemote>> {
        self.tracks.first().map(|t| Arc::clone(&t.track))
    }

    /// tracks returns the RtpTransceiver tracks
    /// A RTPReceiver to support Simulcast may now have multiple tracks
    pub fn tracks(&self) -> Vec<Arc<TrackRemote>> {
        self.tracks.iter().map(|t| Arc::clone(&t.track)).collect()
    }

    /// receive initialize the track and starts all the transports
    pub async fn receive(&mut self, parameters: &RTPReceiveParameters) -> Result<()> {
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
                    self.streams_for_ssrc(encoding.ssrc, &stream_info).await?;

                let t = TrackStreams {
                    track: Arc::new(TrackRemote::new(
                        self.kind,
                        encoding.ssrc,
                        "".to_owned(),
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
                        self.interceptor.clone(),
                    )),
                    ..Default::default()
                });
            }
        }

        Ok(())
    }

    /// read reads incoming RTCP for this RTPReceiver
    pub async fn read(&mut self, b: &mut [u8]) -> Result<(usize, Attributes)> {
        tokio::select! {
            _ = self.received_rx.recv() =>{
                if let Some(rtcp_interceptor) = &self.tracks[0].rtcp_interceptor{
                    let a = Attributes::new();
                    rtcp_interceptor.read(b, &a).await
                }else{
                    Err(Error::ErrInterceptorNotBind.into())
                }
            }
            _ = self.closed_rx.recv() => {
                Err(Error::ErrClosedPipe.into())
            }
        }
    }

    /// read_simulcast reads incoming RTCP for this RTPReceiver for given rid
    pub async fn read_simulcast(&mut self, b: &mut [u8], rid: &str) -> Result<(usize, Attributes)> {
        tokio::select! {
            _ = self.received_rx.recv() =>{
                for  t in &self.tracks {
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
    pub async fn read_rtcp(&mut self) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, attributes) = self.read(&mut b).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }

    /// read_simulcast_rtcp is a convenience method that wraps ReadSimulcast and unmarshal for you
    pub async fn read_simulcast_rtcp(
        &mut self,
        rid: &str,
    ) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, attributes) = self.read_simulcast(&mut b, rid).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }

    fn have_received(&self) -> bool {
        self.received_tx.is_none()
    }

    /// Stop irreversibly stops the RTPReceiver
    pub async fn stop(&mut self) -> Result<()> {
        if self.closed_tx.is_none() {
            return Ok(());
        }
        let _d = self.closed_tx.take();

        tokio::select! {
            _ = self.received_rx.recv() =>{
                let mut errs = None;
                for t in &self.tracks {
                    if let Some(rtcp_read_stream) = &t.rtcp_read_stream{
                        if let Err(err) = rtcp_read_stream.close().await {
                            errs = Some(err);
                        }
                    }

                    if let Some(rtp_read_stream) = &t.rtp_read_stream {
                        if let Err(err) = rtp_read_stream.close().await {
                            errs = Some(err);
                        }
                    }

                    if let Some(interceptor) = &self.interceptor{
                        interceptor.unbind_remote_stream(&t.stream_info).await;
                    }
                }

                if let Some(err) = errs{
                    Err(err)
                }else{
                    Ok(())
                }
            }
            else => {
                Ok(())
            }
        }
    }

    /*TODO:
    async fn streams_for_track(&self, t: &Arc<TrackRemote>) -> Option<&TrackStreams> {
        for track in &self.tracks {
            if &track.track == t {
                return Some(track);
            }
        }
        None
    }

    // readRTP should only be called by a track, this only exists so we can keep state in one place
    func (r *RTPReceiver) readRTP(b []byte, reader *TrackRemote) (n int, a interceptor.Attributes, err error) {
        <-r.received
        if t := r.streamsForTrack(reader); t != nil {
            return t.rtpInterceptor.Read(b, a)
        }

        return 0, nil, fmt.Errorf("%w: %d", errRTPReceiverWithSSRCTrackStreamNotFound, reader.SSRC())
    }*/

    /// receive_for_rid is the sibling of Receive expect for RIDs instead of SSRCs
    /// It populates all the internal state for the given RID
    fn receive_for_rid(
        &mut self,
        rid: &str,
        _params: &RTPParameters,
        _ssrc: SSRC,
    ) -> Result<Arc<TrackRemote>> {
        for t in &mut self.tracks {
            if t.track.rid() == rid {
                /*TODO: t.track.kind = r.kind
                t.track.codec = params.Codecs[0]
                t.track.params = params
                t.track.ssrc = ssrc
                t.streamInfo = createStreamInfo("", ssrc, params.Codecs[0].PayloadType, params.Codecs[0].RTPCodecCapability, params.HeaderExtensions)

                var err error
                if r.tracks[i].rtpReadStream, r.tracks[i].rtpInterceptor, r.tracks[i].rtcpReadStream, r.tracks[i].rtcpInterceptor, err = r.streams_for_ssrc(ssrc, r.tracks[i].streamInfo); err != nil {
                    return nil, err
                }*/

                return Ok(Arc::clone(&t.track));
            }
        }

        Err(Error::ErrRTPReceiverForSSRCTrackStreamNotFound.into())
    }

    async fn streams_for_ssrc(
        &self,
        ssrc: SSRC,
        _stream_info: &StreamInfo,
    ) -> Result<(
        Option<Arc<srtp::stream::Stream>>,
        Option<Box<dyn RTPReader + Send + Sync>>,
        Option<Arc<srtp::stream::Stream>>,
        Option<Box<dyn RTCPReader + Send + Sync>>,
    )> {
        let srtp_session = self
            .transport
            .get_srtp_session()
            .ok_or(Error::ErrDtlsTransportNotStarted)?;
        let rtp_read_stream = srtp_session.listen(ssrc).await?;

        /*TODO: rtp_interceptor := r.api.interceptor.bind_remote_stream(&streamInfo, interceptor.RTPReaderFunc(func(in []byte, a interceptor.Attributes) (n int, attributes interceptor.Attributes, err error) {
            n, err = rtp_read_stream.Read(in)
            return n, a, err
        }))*/
        let rtp_interceptor = None;

        let srtcp_session = self
            .transport
            .get_srtcp_session()
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

    /*TODO:
    // SetReadDeadline sets the max amount of time the RTCP stream will block before returning. 0 is forever.
    func (r *RTPReceiver) SetReadDeadline(t time.Time) error {
        r.mu.RLock()
        defer r.mu.RUnlock()

        if err := r.tracks[0].rtcpReadStream.SetReadDeadline(t); err != nil {
            return err
        }
        return nil
    }

    // SetReadDeadlineSimulcast sets the max amount of time the RTCP stream for a given rid will block before returning. 0 is forever.
    func (r *RTPReceiver) SetReadDeadlineSimulcast(deadline time.Time, rid string) error {
        r.mu.RLock()
        defer r.mu.RUnlock()

        for _, t := range r.tracks {
            if t.track != nil && t.track.rid == rid {
                return t.rtcpReadStream.SetReadDeadline(deadline)
            }
        }
        return fmt.Errorf("%w: %s", errRTPReceiverForRIDTrackStreamNotFound, rid)
    }

    // setRTPReadDeadline sets the max amount of time the RTP stream will block before returning. 0 is forever.
    // This should be fired by calling SetReadDeadline on the TrackRemote
    func (r *RTPReceiver) setRTPReadDeadline(deadline time.Time, reader *TrackRemote) error {
        r.mu.RLock()
        defer r.mu.RUnlock()

        if t := r.streamsForTrack(reader); t != nil {
            return t.rtpReadStream.SetReadDeadline(deadline)
        }
        return fmt.Errorf("%w: %d", errRTPReceiverWithSSRCTrackStreamNotFound, reader.SSRC())
    }
    */
}
