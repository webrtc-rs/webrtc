mod fmtp;
pub mod rtp_codec;

use rtp_codec::*;

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
#[derive(Default, Debug, Clone)]
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
pub struct RTPCapabilities {
    pub codecs: Vec<RTPCodecCapability>,
    pub header_extensions: Vec<RTPHeaderExtensionCapability>,
}
