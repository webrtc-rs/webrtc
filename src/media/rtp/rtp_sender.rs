use crate::api::media_engine::MediaEngine;
use crate::media::dtls_transport::DTLSTransport;
use crate::media::interceptor::stream_info::StreamInfo;
use crate::media::interceptor::RTCPReader;
use crate::media::rtp::rtp_transceiver_direction::RTPTransceiverDirection;
use crate::media::rtp::srtp_writer_future::SrtpWriterFuture;
use crate::media::rtp::{PayloadType, RTPEncodingParameters, RTPSendParameters, SSRC};
use crate::media::track::track_local::{TrackLocal, TrackLocalContext};
//use crate::error::Error;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;

/// RTPSender allows an application to control how a given Track is encoded and transmitted to a remote peer
pub struct RTPSender {
    pub(crate) track: Arc<dyn TrackLocal>,

    pub(crate) srtp_stream: SrtpWriterFuture,
    pub(crate) rtcp_interceptor: Option<Box<dyn RTCPReader>>,
    pub(crate) stream_info: StreamInfo,

    pub(crate) context: TrackLocalContext,

    pub(crate) transport: Arc<DTLSTransport>,

    pub(crate) payload_type: PayloadType,
    pub(crate) ssrc: SSRC,

    /// a transceiver sender since we can just check the
    /// transceiver negotiation status
    pub(crate) negotiated: bool,

    pub(crate) media_engine: Arc<MediaEngine>,

    pub(crate) id: String,

    pub(crate) send_called_tx: Option<mpsc::Sender<()>>,
    pub(crate) send_called_rx: mpsc::Receiver<()>,
    pub(crate) stop_called_tx: Option<mpsc::Sender<()>>,
    pub(crate) stop_called_rx: mpsc::Receiver<()>,
}

impl RTPSender {
    fn is_negotiated(&self) -> bool {
        self.negotiated
    }

    fn set_segotiated(&mut self) {
        self.negotiated = true;
    }

    /// transport returns the currently-configured DTLSTransport
    /// if one has not yet been configured
    pub fn transport(&self) -> Arc<DTLSTransport> {
        Arc::clone(&self.transport)
    }

    /// get_parameters describes the current configuration for the encoding and
    /// transmission of media on the sender's track.
    pub fn get_parameters(&self) -> RTPSendParameters {
        RTPSendParameters {
            rtp_parameters: self.media_engine.get_rtp_parameters_by_kind(
                self.track.kind(),
                &[RTPTransceiverDirection::Sendonly],
            ),
            encodings: vec![RTPEncodingParameters {
                rid: String::new(),
                ssrc: self.ssrc,
                payload_type: self.payload_type,
            }],
        }
    }

    /// track returns the RTCRtpTransceiver track, or nil
    pub fn track(&self) -> Arc<dyn TrackLocal> {
        Arc::clone(&self.track)
    }

    /// replace_track replaces the track currently being used as the sender's source with a new TrackLocal.
    /// The new track must be of the same media kind (audio, video, etc) and switching the track should not
    /// require negotiation.
    pub async fn replace_track(&mut self, track: Arc<dyn TrackLocal>) -> Result<()> {
        if self.has_sent() {
            self.track.unbind(&self.context).await?;
            if let Err(err) = track.bind(&self.context).await {
                // Re-bind the original track
                self.track.bind(&self.context).await?;
                return Err(err);
            }
        }

        self.track = track;

        Ok(())
    }
    /*
    // Send Attempts to set the parameters controlling the sending of media.
    pub async fn Send(&self, parameters:&RTPSendParameters) ->Result<()>{
        if self.has_sent() {
            return Err(Error::ErrRTPSenderSendAlreadyCalled.into());
        }

        writeStream := &interceptorToTrackLocalWriter{}
        r.context = TrackLocalContext{
            id:          r.id,
            params:      r.api.mediaEngine.get_rtpparameters_by_kind(r.track.kind(), []RTPTransceiverDirection{RTPTransceiverDirectionSendonly}),
            ssrc:        parameters.Encodings[0].SSRC,
            writeStream: writeStream,
        }

        codec, err := r.track.bind(r.context)
        if err != nil {
            return err
        }
        r.context.params.Codecs = []RTPCodecParameters{codec}

        r.stream_info = createStreamInfo(r.id, parameters.Encodings[0].SSRC, codec.PayloadType, codec.RTPCodecCapability, parameters.header_extensions)
        rtpInterceptor := r.api.interceptor.bind_local_stream(&r.stream_info, interceptor.RTPWriterFunc(func(header *rtp.Header, payload []byte, attributes interceptor.Attributes) (int, error) {
            return r.srtp_stream.write_rtp(header, payload)
        }))
        writeStream.interceptor.Store(rtpInterceptor)

        close(r.sendCalled)
        return nil
    }

    // Stop irreversibly stops the RTPSender
    func (r *RTPSender) Stop() error {
        r.mu.Lock()

        if stopped := r.has_stopped(); stopped {
            r.mu.Unlock()
            return nil
        }

        close(r.stopCalled)
        r.mu.Unlock()

        if !r.has_sent() {
            return nil
        }

        if err := r.replace_track(nil); err != nil {
            return err
        }

        r.api.interceptor.unbind_local_stream(&r.stream_info)

        return r.srtp_stream.Close()
    }

    // Read reads incoming RTCP for this RTPReceiver
    func (r *RTPSender) Read(b []byte) (n int, a interceptor.Attributes, err error) {
        select {
        case <-r.sendCalled:
            return r.rtcp_interceptor.Read(b, a)
        case <-r.stopCalled:
            return 0, nil, io.ErrClosedPipe
        }
    }

    // read_rtcp is a convenience method that wraps Read and unmarshals for you.
    func (r *RTPSender) read_rtcp() ([]rtcp.Packet, interceptor.Attributes, error) {
        b := make([]byte, receiveMTU)
        i, attributes, err := r.Read(b)
        if err != nil {
            return nil, nil, err
        }

        pkts, err := rtcp.Unmarshal(b[:i])
        if err != nil {
            return nil, nil, err
        }

        return pkts, attributes, nil
    }

    // SetReadDeadline sets the deadline for the Read operation.
    // Setting to zero means no deadline.
    func (r *RTPSender) SetReadDeadline(t time.Time) error {
        return r.srtp_stream.SetReadDeadline(t)
    }
    */

    /// has_sent tells if data has been ever sent for this instance
    fn has_sent(&self) -> bool {
        self.send_called_tx.is_none()
    }

    /// has_stopped tells if stop has been called
    fn has_stopped(&self) -> bool {
        self.stop_called_tx.is_none()
    }
}
