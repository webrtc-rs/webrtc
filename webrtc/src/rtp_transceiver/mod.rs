#[cfg(test)]
mod rtp_transceiver_test;

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use interceptor::stream_info::{AssociatedStreamInfo, RTPHeaderExtension, StreamInfo};
use interceptor::Attributes;
use log::trace;
use portable_atomic::{AtomicBool, AtomicU8};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use tokio::sync::{Mutex, OnceCell};
use util::Unmarshal;

use crate::api::media_engine::MediaEngine;
use crate::error::{Error, Result};
use crate::rtp_transceiver::rtp_codec::*;
use crate::rtp_transceiver::rtp_receiver::{RTCRtpReceiver, RTPReceiverInternal};
use crate::rtp_transceiver::rtp_sender::RTCRtpSender;
use crate::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use crate::track::track_local::TrackLocal;

pub(crate) mod fmtp;
pub mod rtp_codec;
pub mod rtp_receiver;
pub mod rtp_sender;
pub mod rtp_transceiver_direction;
pub(crate) mod srtp_writer_future;

/// SSRC represents a synchronization source
/// A synchronization source is a randomly chosen
/// value meant to be globally unique within a particular
/// RTP session. Used to identify a single stream of media.
/// <https://tools.ietf.org/html/rfc3550#section-3>
#[allow(clippy::upper_case_acronyms)]
pub type SSRC = u32;

/// PayloadType identifies the format of the RTP payload and determines
/// its interpretation by the application. Each codec in a RTP Session
/// will have a different PayloadType
/// <https://tools.ietf.org/html/rfc3550#section-3>
pub type PayloadType = u8;

/// TYPE_RTCP_FBT_RANSPORT_CC ..
pub const TYPE_RTCP_FB_TRANSPORT_CC: &str = "transport-cc";

/// TYPE_RTCP_FB_GOOG_REMB ..
pub const TYPE_RTCP_FB_GOOG_REMB: &str = "goog-remb";

/// TYPE_RTCP_FB_ACK ..
pub const TYPE_RTCP_FB_ACK: &str = "ack";

/// TYPE_RTCP_FB_CCM ..
pub const TYPE_RTCP_FB_CCM: &str = "ccm";

/// TYPE_RTCP_FB_NACK ..
pub const TYPE_RTCP_FB_NACK: &str = "nack";

/// rtcpfeedback signals the connection to use additional RTCP packet types.
/// <https://draft.ortc.org/#dom-rtcrtcpfeedback>
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RTCPFeedback {
    /// Type is the type of feedback.
    /// see: <https://draft.ortc.org/#dom-rtcrtcpfeedback>
    /// valid: ack, ccm, nack, goog-remb, transport-cc
    pub typ: String,

    /// The parameter value depends on the type.
    /// For example, type="nack" parameter="pli" will send Picture Loss Indicator packets.
    pub parameter: String,
}

/// RTPCapabilities represents the capabilities of a transceiver
/// <https://w3c.github.io/webrtc-pc/#rtcrtpcapabilities>
#[derive(Default, Debug, Clone)]
pub struct RTCRtpCapabilities {
    pub codecs: Vec<RTCRtpCodecCapability>,
    pub header_extensions: Vec<RTCRtpHeaderExtensionCapability>,
}

/// RTPRtxParameters dictionary contains information relating to retransmission (RTX) settings.
/// <https://draft.ortc.org/#dom-rtcrtprtxparameters>
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RTCRtpRtxParameters {
    pub ssrc: SSRC,
}

/// RTPCodingParameters provides information relating to both encoding and decoding.
/// This is a subset of the RFC since Pion WebRTC doesn't implement encoding/decoding itself
/// <http://draft.ortc.org/#dom-rtcrtpcodingparameters>
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RTCRtpCodingParameters {
    pub rid: SmolStr,
    pub ssrc: SSRC,
    pub payload_type: PayloadType,
    pub rtx: RTCRtpRtxParameters,
}

/// RTPDecodingParameters provides information relating to both encoding and decoding.
/// This is a subset of the RFC since Pion WebRTC doesn't implement decoding itself
/// <http://draft.ortc.org/#dom-rtcrtpdecodingparameters>
pub type RTCRtpDecodingParameters = RTCRtpCodingParameters;

/// RTPEncodingParameters provides information relating to both encoding and decoding.
/// This is a subset of the RFC since Pion WebRTC doesn't implement encoding itself
/// <http://draft.ortc.org/#dom-rtcrtpencodingparameters>
pub type RTCRtpEncodingParameters = RTCRtpCodingParameters;

/// RTPReceiveParameters contains the RTP stack settings used by receivers
#[derive(Debug)]
pub struct RTCRtpReceiveParameters {
    pub encodings: Vec<RTCRtpDecodingParameters>,
}

/// RTPSendParameters contains the RTP stack settings used by receivers
#[derive(Debug)]
pub struct RTCRtpSendParameters {
    pub rtp_parameters: RTCRtpParameters,
    pub encodings: Vec<RTCRtpEncodingParameters>,
}

/// RTPTransceiverInit dictionary is used when calling the WebRTC function addTransceiver() to provide configuration options for the new transceiver.
pub struct RTCRtpTransceiverInit {
    pub direction: RTCRtpTransceiverDirection,
    pub send_encodings: Vec<RTCRtpEncodingParameters>,
    // Streams       []*Track
}

pub(crate) fn create_stream_info(
    id: String,
    ssrc: SSRC,
    payload_type: PayloadType,
    codec: RTCRtpCodecCapability,
    webrtc_header_extensions: &[RTCRtpHeaderExtensionParameters],
    associated_stream: Option<AssociatedStreamInfo>,
) -> StreamInfo {
    let header_extensions: Vec<RTPHeaderExtension> = webrtc_header_extensions
        .iter()
        .map(|h| RTPHeaderExtension {
            id: h.id,
            uri: h.uri.clone(),
        })
        .collect();

    let feedbacks: Vec<_> = codec
        .rtcp_feedback
        .iter()
        .map(|f| interceptor::stream_info::RTCPFeedback {
            typ: f.typ.clone(),
            parameter: f.parameter.clone(),
        })
        .collect();

    StreamInfo {
        id,
        attributes: Attributes::new(),
        ssrc,
        payload_type,
        rtp_header_extensions: header_extensions,
        mime_type: codec.mime_type,
        clock_rate: codec.clock_rate,
        channels: codec.channels,
        sdp_fmtp_line: codec.sdp_fmtp_line,
        rtcp_feedback: feedbacks,
        associated_stream,
    }
}

pub type TriggerNegotiationNeededFnOption =
    Option<Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send + Sync>> + Send + Sync>>;

/// RTPTransceiver represents a combination of an RTPSender and an RTPReceiver that share a common mid.
pub struct RTCRtpTransceiver {
    mid: OnceCell<SmolStr>,               //atomic.Value
    sender: Mutex<Arc<RTCRtpSender>>,     //atomic.Value
    receiver: Mutex<Arc<RTCRtpReceiver>>, //atomic.Value

    direction: AtomicU8,         //RTPTransceiverDirection
    current_direction: AtomicU8, //RTPTransceiverDirection

    codecs: Arc<Mutex<Vec<RTCRtpCodecParameters>>>, // User provided codecs via set_codec_preferences

    pub(crate) stopped: AtomicBool,
    pub(crate) kind: RTPCodecType,

    media_engine: Arc<MediaEngine>,

    trigger_negotiation_needed: Mutex<TriggerNegotiationNeededFnOption>,
}

impl RTCRtpTransceiver {
    pub async fn new(
        receiver: Arc<RTCRtpReceiver>,
        sender: Arc<RTCRtpSender>,
        direction: RTCRtpTransceiverDirection,
        kind: RTPCodecType,
        codecs: Vec<RTCRtpCodecParameters>,
        media_engine: Arc<MediaEngine>,
        trigger_negotiation_needed: TriggerNegotiationNeededFnOption,
    ) -> Arc<Self> {
        let codecs = Arc::new(Mutex::new(codecs));
        receiver.set_transceiver_codecs(Some(Arc::clone(&codecs)));

        let t = Arc::new(RTCRtpTransceiver {
            mid: OnceCell::new(),
            sender: Mutex::new(sender),
            receiver: Mutex::new(receiver),

            direction: AtomicU8::new(direction as u8),
            current_direction: AtomicU8::new(RTCRtpTransceiverDirection::Unspecified as u8),

            codecs,
            stopped: AtomicBool::new(false),
            kind,
            media_engine,
            trigger_negotiation_needed: Mutex::new(trigger_negotiation_needed),
        });
        t.sender()
            .await
            .set_rtp_transceiver(Some(Arc::downgrade(&t)));

        t
    }

    /// set_codec_preferences sets preferred list of supported codecs
    /// if codecs is empty or nil we reset to default from MediaEngine
    pub async fn set_codec_preferences(&self, codecs: Vec<RTCRtpCodecParameters>) -> Result<()> {
        for codec in &codecs {
            let media_engine_codecs = self.media_engine.get_codecs_by_kind(self.kind);
            let (_, match_type) = codec_parameters_fuzzy_search(codec, &media_engine_codecs);
            if match_type == CodecMatch::None {
                return Err(Error::ErrRTPTransceiverCodecUnsupported);
            }
        }

        {
            let mut c = self.codecs.lock().await;
            *c = codecs;
        }
        Ok(())
    }

    /// Codecs returns list of supported codecs
    pub(crate) async fn get_codecs(&self) -> Vec<RTCRtpCodecParameters> {
        let mut codecs = self.codecs.lock().await;
        RTPReceiverInternal::get_codecs(&mut codecs, self.kind, &self.media_engine)
    }

    /// sender returns the RTPTransceiver's RTPSender if it has one
    pub async fn sender(&self) -> Arc<RTCRtpSender> {
        let sender = self.sender.lock().await;
        sender.clone()
    }

    /// set_sender_track sets the RTPSender and Track to current transceiver
    pub async fn set_sender_track(
        self: &Arc<Self>,
        sender: Arc<RTCRtpSender>,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        self.set_sender(sender).await;
        self.set_sending_track(track).await
    }

    pub async fn set_sender(self: &Arc<Self>, s: Arc<RTCRtpSender>) {
        s.set_rtp_transceiver(Some(Arc::downgrade(self)));

        let prev_sender = self.sender().await;
        prev_sender.set_rtp_transceiver(None);

        {
            let mut sender = self.sender.lock().await;
            *sender = s;
        }
    }

    /// receiver returns the RTPTransceiver's RTPReceiver if it has one
    pub async fn receiver(&self) -> Arc<RTCRtpReceiver> {
        let receiver = self.receiver.lock().await;
        receiver.clone()
    }

    pub(crate) async fn set_receiver(&self, r: Arc<RTCRtpReceiver>) {
        r.set_transceiver_codecs(Some(Arc::clone(&self.codecs)));

        {
            let mut receiver = self.receiver.lock().await;
            (*receiver).set_transceiver_codecs(None);

            *receiver = r;
        }
    }

    /// set_mid sets the RTPTransceiver's mid. If it was already set, will return an error.
    pub(crate) fn set_mid(&self, mid: SmolStr) -> Result<()> {
        self.mid
            .set(mid)
            .map_err(|_| Error::ErrRTPTransceiverCannotChangeMid)
    }

    /// mid gets the Transceiver's mid value. When not already set, this value will be set in CreateOffer or create_answer.
    pub fn mid(&self) -> Option<SmolStr> {
        self.mid.get().cloned()
    }

    /// kind returns RTPTransceiver's kind.
    pub fn kind(&self) -> RTPCodecType {
        self.kind
    }

    /// direction returns the RTPTransceiver's desired direction.
    pub fn direction(&self) -> RTCRtpTransceiverDirection {
        self.direction.load(Ordering::SeqCst).into()
    }

    /// Set the direction of this transceiver. This might trigger a renegotiation.
    pub async fn set_direction(&self, d: RTCRtpTransceiverDirection) {
        let changed = self.set_direction_internal(d);

        if changed {
            let lock = self.trigger_negotiation_needed.lock().await;
            if let Some(trigger) = &*lock {
                (trigger)().await;
            }
        }
    }

    pub(crate) fn set_direction_internal(&self, d: RTCRtpTransceiverDirection) -> bool {
        let previous: RTCRtpTransceiverDirection =
            self.direction.swap(d as u8, Ordering::SeqCst).into();

        let changed = d != previous;

        if changed {
            trace!(
                "Changing direction of transceiver from {} to {}",
                previous,
                d
            );
        }

        changed
    }

    /// current_direction returns the RTPTransceiver's current direction as negotiated.
    ///
    /// If this transceiver has never been negotiated or if it's stopped this returns [`RTCRtpTransceiverDirection::Unspecified`].
    pub fn current_direction(&self) -> RTCRtpTransceiverDirection {
        if self.stopped.load(Ordering::SeqCst) {
            return RTCRtpTransceiverDirection::Unspecified;
        }

        self.current_direction.load(Ordering::SeqCst).into()
    }

    pub(crate) fn set_current_direction(&self, d: RTCRtpTransceiverDirection) {
        let previous: RTCRtpTransceiverDirection = self
            .current_direction
            .swap(d as u8, Ordering::SeqCst)
            .into();

        if d != previous {
            trace!(
                "Changing current direction of transceiver from {} to {}",
                previous,
                d,
            );
        }
    }

    /// Perform any subsequent actions after altering the transceiver's direction.
    ///
    /// After changing the transceiver's direction this method should be called to perform any
    /// side-effects that results from the new direction, such as pausing/resuming the RTP receiver.
    pub(crate) async fn process_new_current_direction(
        &self,
        previous_direction: RTCRtpTransceiverDirection,
    ) -> Result<()> {
        if self.stopped.load(Ordering::SeqCst) {
            return Ok(());
        }

        let current_direction = self.current_direction();
        if previous_direction != current_direction {
            let mid = self.mid();
            trace!(
                "Processing transceiver({:?}) direction change from {} to {}",
                mid,
                previous_direction,
                current_direction
            );
        } else {
            // no change.
            return Ok(());
        }

        {
            let receiver = self.receiver.lock().await;
            let pause_receiver = !current_direction.has_recv();

            if pause_receiver {
                receiver.pause().await?;
            } else {
                receiver.resume().await?;
            }
        }

        let pause_sender = !current_direction.has_send();
        {
            let sender = &*self.sender.lock().await;
            sender.set_paused(pause_sender);
        }

        Ok(())
    }

    /// stop irreversibly stops the RTPTransceiver
    pub async fn stop(&self) -> Result<()> {
        if self.stopped.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.stopped.store(true, Ordering::SeqCst);

        {
            let sender = self.sender.lock().await;
            sender.stop().await?;
        }
        {
            let r = self.receiver.lock().await;
            r.stop().await?;
        }

        self.set_direction_internal(RTCRtpTransceiverDirection::Inactive);

        Ok(())
    }

    pub(crate) async fn set_sending_track(
        &self,
        track: Option<Arc<dyn TrackLocal + Send + Sync>>,
    ) -> Result<()> {
        let track_is_none = track.is_none();
        {
            let sender = self.sender.lock().await;
            sender.replace_track(track).await?;
        }

        let direction = self.direction();
        let should_send = !track_is_none;
        let should_recv = direction.has_recv();
        self.set_direction_internal(RTCRtpTransceiverDirection::from_send_recv(
            should_send,
            should_recv,
        ));

        Ok(())
    }
}

impl fmt::Debug for RTCRtpTransceiver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RTCRtpTransceiver")
            .field("mid", &self.mid)
            .field("sender", &self.sender)
            .field("receiver", &self.receiver)
            .field("direction", &self.direction)
            .field("current_direction", &self.current_direction)
            .field("codecs", &self.codecs)
            .field("stopped", &self.stopped)
            .field("kind", &self.kind)
            .finish()
    }
}

pub(crate) async fn find_by_mid(
    mid: &str,
    local_transceivers: &mut Vec<Arc<RTCRtpTransceiver>>,
) -> Option<Arc<RTCRtpTransceiver>> {
    for (i, t) in local_transceivers.iter().enumerate() {
        if t.mid() == Some(SmolStr::from(mid)) {
            return Some(local_transceivers.remove(i));
        }
    }

    None
}

/// Given a direction+type pluck a transceiver from the passed list
/// if no entry satisfies the requested type+direction return a inactive Transceiver
pub(crate) async fn satisfy_type_and_direction(
    remote_kind: RTPCodecType,
    remote_direction: RTCRtpTransceiverDirection,
    local_transceivers: &mut Vec<Arc<RTCRtpTransceiver>>,
) -> Option<Arc<RTCRtpTransceiver>> {
    // Get direction order from most preferred to least
    let get_preferred_directions = || -> Vec<RTCRtpTransceiverDirection> {
        match remote_direction {
            RTCRtpTransceiverDirection::Sendrecv => vec![
                RTCRtpTransceiverDirection::Recvonly,
                RTCRtpTransceiverDirection::Sendrecv,
            ],
            RTCRtpTransceiverDirection::Sendonly => vec![RTCRtpTransceiverDirection::Recvonly],
            RTCRtpTransceiverDirection::Recvonly => vec![
                RTCRtpTransceiverDirection::Sendonly,
                RTCRtpTransceiverDirection::Sendrecv,
            ],
            _ => vec![],
        }
    };

    for possible_direction in get_preferred_directions() {
        for (i, t) in local_transceivers.iter().enumerate() {
            if t.mid().is_none() && t.kind == remote_kind && possible_direction == t.direction() {
                return Some(local_transceivers.remove(i));
            }
        }
    }

    None
}

/// handle_unknown_rtp_packet consumes a single RTP Packet and returns information that is helpful
/// for demuxing and handling an unknown SSRC (usually for Simulcast)
pub(crate) fn handle_unknown_rtp_packet(
    buf: &[u8],
    mid_extension_id: u8,
    sid_extension_id: u8,
    rsid_extension_id: u8,
) -> Result<(String, String, String, PayloadType)> {
    let mut reader = buf;
    let rp = rtp::packet::Packet::unmarshal(&mut reader)?;

    if !rp.header.extension {
        return Ok((String::new(), String::new(), String::new(), 0));
    }

    let payload_type = rp.header.payload_type;

    let mid = if let Some(payload) = rp.header.get_extension(mid_extension_id) {
        String::from_utf8(payload.to_vec())?
    } else {
        String::new()
    };

    let rid = if let Some(payload) = rp.header.get_extension(sid_extension_id) {
        String::from_utf8(payload.to_vec())?
    } else {
        String::new()
    };

    let srid = if let Some(payload) = rp.header.get_extension(rsid_extension_id) {
        String::from_utf8(payload.to_vec())?
    } else {
        String::new()
    };

    Ok((mid, rid, srid, payload_type))
}
