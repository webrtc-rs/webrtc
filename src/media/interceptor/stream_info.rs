use super::Attributes;

/// RTPHeaderExtension represents a negotiated RFC5285 RTP header extension.
pub struct RTPHeaderExtension {
    uri: String,
    id: usize,
}

/// RTCPFeedback signals the connection to use additional RTCP packet types.
/// https://draft.ortc.org/#dom-rtcrtcpfeedback
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
pub struct StreamInfo {
    id: String,
    attributes: Attributes,
    ssrc: u32,
    payload_type: u8,
    rtp_header_extensions: Vec<RTPHeaderExtension>,
    mime_type: String,
    clock_rate: u32,
    channels: u16,
    sdp_fmtp_line: String,
    rtcp_feedback: Vec<RTCPFeedback>,
}
