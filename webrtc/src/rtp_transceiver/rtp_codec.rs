use std::fmt;

use super::*;
use crate::api::media_engine::*;
use crate::error::{Error, Result};
use crate::rtp_transceiver::fmtp;

/// RTPCodecType determines the type of a codec
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTPCodecType {
    #[default]
    Unspecified = 0,

    /// RTPCodecTypeAudio indicates this is an audio codec
    Audio = 1,

    /// RTPCodecTypeVideo indicates this is a video codec
    Video = 2,
}

impl From<&str> for RTPCodecType {
    fn from(raw: &str) -> Self {
        match raw {
            "audio" => RTPCodecType::Audio,
            "video" => RTPCodecType::Video,
            _ => RTPCodecType::Unspecified,
        }
    }
}

impl From<u8> for RTPCodecType {
    fn from(v: u8) -> Self {
        match v {
            1 => RTPCodecType::Audio,
            2 => RTPCodecType::Video,
            _ => RTPCodecType::Unspecified,
        }
    }
}

impl fmt::Display for RTPCodecType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTPCodecType::Audio => "audio",
            RTPCodecType::Video => "video",
            RTPCodecType::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{s}")
    }
}

/// RTPCodecCapability provides information about codec capabilities.
///
/// ## Specifications
///
/// * [W3C]
///
/// [W3C]: https://w3c.github.io/webrtc-pc/#dictionary-rtcrtpcodeccapability-members
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RTCRtpCodecCapability {
    pub mime_type: String,
    pub clock_rate: u32,
    pub channels: u16,
    pub sdp_fmtp_line: String,
    pub rtcp_feedback: Vec<RTCPFeedback>,
}

impl RTCRtpCodecCapability {
    /// Turn codec capability into a `packetizer::Payloader`
    pub fn payloader_for_codec(&self) -> Result<Box<dyn rtp::packetizer::Payloader + Send + Sync>> {
        let mime_type = self.mime_type.to_lowercase();
        if mime_type == MIME_TYPE_H264.to_lowercase() {
            Ok(Box::<rtp::codecs::h264::H264Payloader>::default())
        } else if mime_type == MIME_TYPE_HEVC.to_lowercase() {
            Ok(Box::<rtp::codecs::h265::HevcPayloader>::default())
        } else if mime_type == MIME_TYPE_VP8.to_lowercase() {
            let mut vp8_payloader = rtp::codecs::vp8::Vp8Payloader::default();
            vp8_payloader.enable_picture_id = true;
            Ok(Box::new(vp8_payloader))
        } else if mime_type == MIME_TYPE_VP9.to_lowercase() {
            Ok(Box::<rtp::codecs::vp9::Vp9Payloader>::default())
        } else if mime_type == MIME_TYPE_OPUS.to_lowercase() {
            Ok(Box::<rtp::codecs::opus::OpusPayloader>::default())
        } else if mime_type == MIME_TYPE_G722.to_lowercase()
            || mime_type == MIME_TYPE_PCMU.to_lowercase()
            || mime_type == MIME_TYPE_PCMA.to_lowercase()
            || mime_type == MIME_TYPE_TELEPHONE_EVENT.to_lowercase()
        {
            Ok(Box::<rtp::codecs::g7xx::G7xxPayloader>::default())
        } else if mime_type == MIME_TYPE_AV1.to_lowercase() {
            Ok(Box::<rtp::codecs::av1::Av1Payloader>::default())
        } else {
            Err(Error::ErrNoPayloaderForCodec)
        }
    }
}

/// RTPHeaderExtensionCapability is used to define a RFC5285 RTP header extension supported by the codec.
/// <https://w3c.github.io/webrtc-pc/#dom-rtcrtpcapabilities-headerextensions>
#[derive(Default, Debug, Clone)]
pub struct RTCRtpHeaderExtensionCapability {
    pub uri: String,
}

/// RTPHeaderExtensionParameter represents a negotiated RFC5285 RTP header extension.
/// <https://w3c.github.io/webrtc-pc/#dictionary-rtcrtpheaderextensionparameters-members>
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RTCRtpHeaderExtensionParameters {
    pub uri: String,
    pub id: isize,
}

/// RTPCodecParameters is a sequence containing the media codecs that an RtpSender
/// will choose from, as well as entries for RTX, RED and FEC mechanisms. This also
/// includes the PayloadType that has been negotiated
/// <https://w3c.github.io/webrtc-pc/#rtcrtpcodecparameters>
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RTCRtpCodecParameters {
    pub capability: RTCRtpCodecCapability,
    pub payload_type: PayloadType,
    pub stats_id: String,
}

/// RTPParameters is a list of negotiated codecs and header extensions
/// <https://w3c.github.io/webrtc-pc/#dictionary-rtcrtpparameters-members>
#[derive(Default, Debug, Clone)]
pub struct RTCRtpParameters {
    pub header_extensions: Vec<RTCRtpHeaderExtensionParameters>,
    pub codecs: Vec<RTCRtpCodecParameters>,
}

#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub(crate) enum CodecMatch {
    #[default]
    None = 0,
    Partial = 1,
    Exact = 2,
}

/// Do a fuzzy find for a codec in the list of codecs
/// Used for lookup up a codec in an existing list to find a match
/// Returns codecMatchExact, codecMatchPartial, or codecMatchNone
pub(crate) fn codec_parameters_fuzzy_search(
    needle: &RTCRtpCodecParameters,
    haystack: &[RTCRtpCodecParameters],
) -> (RTCRtpCodecParameters, CodecMatch) {
    let needle_fmtp = fmtp::parse(
        &needle.capability.mime_type,
        &needle.capability.sdp_fmtp_line,
    );

    //TODO: add unicode case-folding equal support

    // First attempt to match on mime_type + sdpfmtp_line
    for c in haystack {
        let cfmpt = fmtp::parse(&c.capability.mime_type, &c.capability.sdp_fmtp_line);
        if needle_fmtp.match_fmtp(&*cfmpt) {
            return (c.clone(), CodecMatch::Exact);
        }
    }

    // Fallback to just mime_type
    for c in haystack {
        if c.capability.mime_type.to_uppercase() == needle.capability.mime_type.to_uppercase() {
            return (c.clone(), CodecMatch::Partial);
        }
    }

    (RTCRtpCodecParameters::default(), CodecMatch::None)
}

pub(crate) fn codec_rtx_search(
    original_codec: &RTCRtpCodecParameters,
    available_codecs: &[RTCRtpCodecParameters],
) -> Option<RTCRtpCodecParameters> {
    // find the rtx codec as defined in RFC4588

    let (mime_kind, _) = original_codec.capability.mime_type.split_once("/")?;
    let rtx_mime = format!("{mime_kind}/rtx");

    for codec in available_codecs {
        if codec.capability.mime_type != rtx_mime {
            continue;
        }

        let params = fmtp::parse(&codec.capability.mime_type, &codec.capability.sdp_fmtp_line);

        if params
            .parameter("apt")
            .and_then(|v| v.parse::<u8>().ok())
            .is_some_and(|apt| apt == original_codec.payload_type)
        {
            return Some(codec.clone());
        }
    }

    None
}
