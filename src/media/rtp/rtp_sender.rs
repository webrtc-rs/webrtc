use crate::api::media_engine::MediaEngine;
use crate::error::Error;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::interceptor::stream_info::StreamInfo;
use crate::media::interceptor::{
    Attributes, Interceptor, InterceptorToTrackLocalWriter, RTCPReader,
};
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::rtp::srtp_writer_future::SrtpWriterFuture;
use crate::media::rtp::{PayloadType, RTPEncodingParameters, RTPSendParameters, SSRC};
use crate::media::track::track_local::{TrackLocal, TrackLocalContext};

use crate::media::rtp::rtp_codec::{RTPCodecParameters, RTPCodecType};
use crate::RECEIVE_MTU;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// RTPSender allows an application to control how a given Track is encoded and transmitted to a remote peer
pub struct RTPSender {
    pub(crate) track: Option<Arc<dyn TrackLocal + Send + Sync>>,

    pub(crate) srtp_stream: SrtpWriterFuture,
    pub(crate) rtcp_interceptor: Option<Box<dyn RTCPReader + Send + Sync>>,
    pub(crate) stream_info: StreamInfo,

    pub(crate) context: TrackLocalContext,

    pub(crate) transport: Arc<DTLSTransport>,

    pub(crate) payload_type: PayloadType,
    pub(crate) ssrc: SSRC,

    /// a transceiver sender since we can just check the
    /// transceiver negotiation status
    pub(crate) negotiated: AtomicBool,

    pub(crate) media_engine: Arc<MediaEngine>,
    pub(crate) interceptor: Option<Arc<dyn Interceptor + Send + Sync>>,

    pub(crate) id: String,

    pub(crate) send_called_tx: Option<mpsc::Sender<()>>,
    pub(crate) send_called_rx: mpsc::Receiver<()>,
    pub(crate) stop_called_tx: Option<mpsc::Sender<()>>,
    pub(crate) stop_called_rx: mpsc::Receiver<()>,
}

impl RTPSender {
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
        RTPSendParameters {
            rtp_parameters: self
                .media_engine
                .get_rtp_parameters_by_kind(
                    if let Some(t) = &self.track {
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
    pub fn track(&self) -> Option<Arc<dyn TrackLocal + Send + Sync>> {
        self.track.clone()
    }

    /// replace_track replaces the track currently being used as the sender's source with a new TrackLocal.
    /// The new track must be of the same media kind (audio, video, etc) and switching the track should not
    /// require negotiation.
    pub async fn replace_track(
        &mut self,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        if self.has_sent() {
            if let Some(track) = &self.track {
                track.unbind(&self.context).await?;
            }
        }

        if !self.has_sent() || track.is_none() {
            self.track = track;
            return Ok(());
        }

        let result = if let Some(t) = &track {
            // Re-bind the original track
            t.bind(&self.context).await
        } else {
            Err(Error::ErrRTPSenderTrackNil.into())
        };

        if let Err(err) = result {
            return Err(err);
        }

        self.track = track;

        Ok(())
    }

    // send Attempts to set the parameters controlling the sending of media.
    pub async fn send(&mut self, parameters: &RTPSendParameters) -> Result<()> {
        if self.has_sent() {
            return Err(Error::ErrRTPSenderSendAlreadyCalled.into());
        }

        self.context = TrackLocalContext {
            id: self.id.clone(),
            params: self
                .media_engine
                .get_rtp_parameters_by_kind(
                    if let Some(t) = &self.track {
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

        let codec = if let Some(t) = &self.track {
            t.bind(&self.context).await?
        } else {
            RTPCodecParameters::default()
        };
        let payload_type = codec.payload_type;
        let capability = codec.capability.clone();

        self.context.params.codecs = vec![codec];

        self.stream_info = StreamInfo::new(
            self.id.clone(),
            parameters.encodings[0].ssrc,
            payload_type,
            capability,
            &parameters.rtp_parameters.header_extensions,
        );
        /*TODO: rtpInterceptor := r.api.interceptor.bind_local_stream(&r.stream_info, interceptor.RTPWriterFunc(func(header *rtp.Header, payload []byte, attributes interceptor.Attributes) (int, error) {
            return r.srtp_stream.write_rtp(header, payload)
        }))
        writeStream.interceptor.Store(rtpInterceptor)*/

        self.send_called_tx.take();

        Ok(())
    }

    /// stop irreversibly stops the RTPSender
    pub async fn stop(&mut self) -> Result<()> {
        if self.has_stopped() {
            return Ok(());
        }

        self.stop_called_tx.take();

        if !self.has_sent() {
            return Ok(());
        }

        self.replace_track(None).await?;

        if let Some(interceptor) = &self.interceptor {
            interceptor.unbind_local_stream(&self.stream_info).await;
        }

        self.srtp_stream.close()
    }

    /// read reads incoming RTCP for this RTPReceiver
    pub async fn read(&mut self, b: &mut [u8]) -> Result<(usize, Attributes)> {
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
    pub async fn read_rtcp(&mut self) -> Result<(Box<dyn rtcp::packet::Packet>, Attributes)> {
        let mut b = vec![0u8; RECEIVE_MTU];
        let (n, attributes) = self.read(&mut b).await?;

        let mut buf = &b[..n];
        let pkts = rtcp::packet::unmarshal(&mut buf)?;

        Ok((pkts, attributes))
    }

    /*
    // SetReadDeadline sets the deadline for the Read operation.
    // Setting to zero means no deadline.
    func (r *RTPSender) SetReadDeadline(t time.Time) error {
        return r.srtp_stream.SetReadDeadline(t)
    }
    */

    /// has_sent tells if data has been ever sent for this instance
    pub(crate) fn has_sent(&self) -> bool {
        self.send_called_tx.is_none()
    }

    /// has_stopped tells if stop has been called
    pub(crate) fn has_stopped(&self) -> bool {
        self.stop_called_tx.is_none()
    }
}
