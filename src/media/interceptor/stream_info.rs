use super::Attributes;
use crate::media::rtp::rtp_codec::{RTPCodecCapability, RTPHeaderExtensionParameter};
use crate::media::rtp::{PayloadType, SSRC};

/// RTPHeaderExtension represents a negotiated RFC5285 RTP header extension.
#[derive(Default, Debug, Clone)]
pub struct RTPHeaderExtension {
    uri: String,
    id: usize,
}

/// RTCPFeedback signals the connection to use additional RTCP packet types.
/// https://draft.ortc.org/#dom-rtcrtcpfeedback
#[derive(Default, Debug, Clone)]
pub struct RTCPFeedback {
    /// Type is the type of feedback.
    /// see: https://draft.ortc.org/#dom-rtcrtcpfeedback
    /// valid: ack, ccm, nack, goog-remb, transport-cc
    typ: String,

    /// The parameter value depends on the type.
    /// For example, type="nack" parameter="pli" will send Picture Loss Indicator packets.
    parameter: String,
}

/// StreamInfo is the Context passed when a StreamLocal or StreamRemote has been Binded or Unbinded
#[derive(Default, Debug, Clone)]
pub struct StreamInfo {
    id: String,
    attributes: Attributes,
    ssrc: u32,
    payload_type: PayloadType,
    rtp_header_extensions: Vec<RTPHeaderExtension>,
    mime_type: String,
    clock_rate: u32,
    channels: u16,
    sdp_fmtp_line: String,
    rtcp_feedback: Vec<RTCPFeedback>,
}

impl StreamInfo {
    pub(crate) fn new(
        id: String,
        ssrc: SSRC,
        payload_type: PayloadType,
        codec: RTPCodecCapability,
        webrtc_header_extensions: &[RTPHeaderExtensionParameter],
    ) -> Self {
        let mut header_extensions = vec![];
        for h in webrtc_header_extensions {
            header_extensions.push(RTPHeaderExtension {
                id: h.id,
                uri: h.uri.clone(),
            });
        }

        let mut feedbacks = vec![];
        for f in &codec.rtcp_feedback {
            feedbacks.push(RTCPFeedback {
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
}
