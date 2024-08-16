use crate::Attributes;

/// RTPHeaderExtension represents a negotiated RFC5285 RTP header extension.
#[derive(Default, Debug, Clone)]
pub struct RTPHeaderExtension {
    pub uri: String,
    pub id: isize,
}

/// StreamInfo is the Context passed when a StreamLocal or StreamRemote has been Binded or Unbinded
#[derive(Default, Debug, Clone)]
pub struct StreamInfo {
    pub id: String,
    pub attributes: Attributes,
    pub ssrc: u32,
    pub payload_type: u8,
    pub rtp_header_extensions: Vec<RTPHeaderExtension>,
    pub mime_type: String,
    pub clock_rate: u32,
    pub channels: u16,
    pub sdp_fmtp_line: String,
    pub rtcp_feedback: Vec<RTCPFeedback>,
    pub associated_stream: Option<AssociatedStreamInfo>,
}

/// AssociatedStreamInfo provides a mapping from an auxiliary stream (RTX, FEC,
/// etc.) back to the original stream.
#[derive(Default, Debug, Clone)]
pub struct AssociatedStreamInfo {
    pub ssrc: u32,
    pub payload_type: u8,
}

/// RTCPFeedback signals the connection to use additional RTCP packet types.
/// <https://draft.ortc.org/#dom-rtcrtcpfeedback>
#[derive(Default, Debug, Clone)]
pub struct RTCPFeedback {
    /// Type is the type of feedback.
    /// see: <https://draft.ortc.org/#dom-rtcrtcpfeedback>
    /// valid: ack, ccm, nack, goog-remb, transport-cc
    pub typ: String,

    /// The parameter value depends on the type.
    /// For example, type="nack" parameter="pli" will send Picture Loss Indicator packets.
    pub parameter: String,
}
