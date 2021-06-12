use super::*;
use crate::rtp::fmtp::*;

use std::fmt;

/// RTPCodecType determines the type of a codec
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum RTPCodecType {
    Unspecified = 0,

    /// RTPCodecTypeAudio indicates this is an audio codec
    Audio = 1,

    /// RTPCodecTypeVideo indicates this is a video codec
    Video = 2,
}

impl Default for RTPCodecType {
    fn default() -> Self {
        RTPCodecType::Unspecified
    }
}

impl From<&str> for RTPCodecType {
    fn from(raw: &str) -> Self {
        match raw {
            "Audio" => RTPCodecType::Audio,
            "Video" => RTPCodecType::Video,
            _ => RTPCodecType::Unspecified,
        }
    }
}

impl fmt::Display for RTPCodecType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTPCodecType::Audio => "Audio",
            RTPCodecType::Video => "Video",
            RTPCodecType::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}

/// RTPCodecCapability provides information about codec capabilities.
/// https://w3c.github.io/webrtc-pc/#dictionary-rtcrtpcodeccapability-members
#[derive(Default, Debug, Clone)]
pub struct RTPCodecCapability {
    pub mime_type: String,
    pub clock_rate: u32,
    pub channels: u16,
    pub sdp_fmtp_line: String,
    pub rtcp_feedback: Vec<RTCPFeedback>,
}

/// RTPHeaderExtensionCapability is used to define a RFC5285 RTP header extension supported by the codec.
/// https://w3c.github.io/webrtc-pc/#dom-rtcrtpcapabilities-headerextensions
#[derive(Default, Debug, Clone)]
pub struct RTPHeaderExtensionCapability {
    pub uri: String,
}

/// RTPHeaderExtensionParameter represents a negotiated RFC5285 RTP header extension.
/// https://w3c.github.io/webrtc-pc/#dictionary-rtcrtpheaderextensionparameters-members
#[derive(Default, Debug, Clone)]
pub struct RTPHeaderExtensionParameter {
    pub uri: String,
    pub id: usize,
}

/// RTPCodecParameters is a sequence containing the media codecs that an RtpSender
/// will choose from, as well as entries for RTX, RED and FEC mechanisms. This also
/// includes the PayloadType that has been negotiated
/// https://w3c.github.io/webrtc-pc/#rtcrtpcodecparameters
#[derive(Default, Debug, Clone)]
pub struct RTPCodecParameters {
    pub capability: RTPCodecCapability,
    pub payload_type: PayloadType,
    pub stats_id: String,
}

/// RTPParameters is a list of negotiated codecs and header extensions
/// https://w3c.github.io/webrtc-pc/#dictionary-rtcrtpparameters-members
pub struct RTPParameters {
    pub header_extensions: Vec<RTPHeaderExtensionParameter>,
    pub codecs: Vec<RTPCodecParameters>,
}

pub(crate) enum CodecMatchType {
    None = 0,
    Partial = 1,
    Exact = 2,
}

/// Do a fuzzy find for a codec in the list of codecs
/// Used for lookup up a codec in an existing list to find a match
/// Returns codecMatchExact, codecMatchPartial, or codecMatchNone
pub(crate) fn codec_parameters_fuzzy_search(
    needle: &RTPCodecParameters,
    haystack: &[RTPCodecParameters],
) -> (RTPCodecParameters, CodecMatchType) {
    let needle_fmtp = parse_fmtp(&needle.capability.sdp_fmtp_line);

    //TODO: do case-folding equal

    // First attempt to match on mime_type + sdpfmtp_line
    for c in haystack {
        if c.capability.mime_type.to_uppercase() == needle.capability.mime_type.to_uppercase()
            && fmtp_consist(&needle_fmtp, &parse_fmtp(&c.capability.sdp_fmtp_line))
        {
            return (c.clone(), CodecMatchType::Exact);
        }
    }

    // Fallback to just mime_type
    for c in haystack {
        if c.capability.mime_type.to_uppercase() == needle.capability.mime_type.to_uppercase() {
            return (c.clone(), CodecMatchType::Partial);
        }
    }

    (RTPCodecParameters::default(), CodecMatchType::None)
}
