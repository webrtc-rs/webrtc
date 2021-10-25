use serde::{Deserialize, Serialize};

use crate::media::rtp::rtp_transceiver_direction::RTCRtpTransceiverDirection;

use interceptor::{
    stream_info::{RTPHeaderExtension, StreamInfo},
    Attributes,
};
use rtp_codec::*;

pub(crate) mod fmtp;
pub mod rtp_codec;
pub mod rtp_receiver;
pub mod rtp_sender;
pub mod rtp_transceiver;
pub mod rtp_transceiver_direction;
pub(crate) mod srtp_writer_future;

/// SSRC represents a synchronization source
/// A synchronization source is a randomly chosen
/// value meant to be globally unique within a particular
/// RTP session. Used to identify a single stream of media.
/// https://tools.ietf.org/html/rfc3550#section-3
#[allow(clippy::upper_case_acronyms)]
pub type SSRC = u32;

/// PayloadType identifies the format of the RTP payload and determines
/// its interpretation by the application. Each codec in a RTP Session
/// will have a different PayloadType
/// https://tools.ietf.org/html/rfc3550#section-3
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
/// https://draft.ortc.org/#dom-rtcrtcpfeedback
#[derive(Default, Debug, Clone, PartialEq)]
pub struct RTCPFeedback {
    /// Type is the type of feedback.
    /// see: https://draft.ortc.org/#dom-rtcrtcpfeedback
    /// valid: ack, ccm, nack, goog-remb, transport-cc
    pub typ: String,

    /// The parameter value depends on the type.
    /// For example, type="nack" parameter="pli" will send Picture Loss Indicator packets.
    pub parameter: String,
}

/// RTPCapabilities represents the capabilities of a transceiver
/// https://w3c.github.io/webrtc-pc/#rtcrtpcapabilities
#[derive(Default, Debug, Clone)]
pub struct RTCRtpCapabilities {
    pub codecs: Vec<RTCRtpCodecCapability>,
    pub header_extensions: Vec<RTCRtpHeaderExtensionCapability>,
}

/// RTPCodingParameters provides information relating to both encoding and decoding.
/// This is a subset of the RFC since Pion WebRTC doesn't implement encoding/decoding itself
/// http://draft.ortc.org/#dom-rtcrtpcodingparameters
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RTCRtpCodingParameters {
    pub rid: String,
    pub ssrc: SSRC,
    pub payload_type: PayloadType,
}

/// RTPDecodingParameters provides information relating to both encoding and decoding.
/// This is a subset of the RFC since Pion WebRTC doesn't implement decoding itself
/// http://draft.ortc.org/#dom-rtcrtpdecodingparameters
pub type RTCRtpDecodingParameters = RTCRtpCodingParameters;

/// RTPEncodingParameters provides information relating to both encoding and decoding.
/// This is a subset of the RFC since Pion WebRTC doesn't implement encoding itself
/// http://draft.ortc.org/#dom-rtcrtpencodingparameters
pub type RTCRtpEncodingParameters = RTCRtpCodingParameters;

/// RTPReceiveParameters contains the RTP stack settings used by receivers
pub struct RTCRtpReceiveParameters {
    pub encodings: Vec<RTCRtpDecodingParameters>,
}

/// RTPSendParameters contains the RTP stack settings used by receivers
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
) -> StreamInfo {
    let mut header_extensions = vec![];
    for h in webrtc_header_extensions {
        header_extensions.push(RTPHeaderExtension {
            id: h.id,
            uri: h.uri.clone(),
        });
    }

    let mut feedbacks = vec![];
    for f in &codec.rtcp_feedback {
        feedbacks.push(interceptor::stream_info::RTCPFeedback {
            typ: f.typ.clone(),
            parameter: f.parameter.clone(),
        });
    }

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
    }
}
