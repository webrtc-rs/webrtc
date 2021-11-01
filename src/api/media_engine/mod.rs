#[cfg(test)]
mod media_engine_test;

use crate::error::{Error, Result};
use crate::peer_connection::sdp::{
    codecs_from_media_description, rtp_extensions_from_media_description,
};
use crate::rtp_transceiver::fmtp;
use crate::rtp_transceiver::rtp_codec::{
    codec_parameters_fuzzy_search, CodecMatch, RTCRtpCodecCapability, RTCRtpCodecParameters,
    RTCRtpHeaderExtensionCapability, RTCRtpHeaderExtensionParameters, RTCRtpParameters,
    RTPCodecType,
};
use crate::rtp_transceiver::rtp_transceiver_direction::{
    have_rtp_transceiver_direction_intersection, RTCRtpTransceiverDirection,
};
use crate::rtp_transceiver::{PayloadType, RTCPFeedback};

use sdp::description::session::SessionDescription;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// MIME_TYPE_H264 H264 MIME type.
/// Note: Matching should be case insensitive.
pub const MIME_TYPE_H264: &str = "video/H264";
/// MIME_TYPE_OPUS Opus MIME type
/// Note: Matching should be case insensitive.
pub const MIME_TYPE_OPUS: &str = "audio/opus";
/// MIME_TYPE_VP8 VP8 MIME type
/// Note: Matching should be case insensitive.
pub const MIME_TYPE_VP8: &str = "video/VP8";
/// MIME_TYPE_VP9 VP9 MIME type
/// Note: Matching should be case insensitive.
pub const MIME_TYPE_VP9: &str = "video/VP9";
/// MIME_TYPE_G722 G722 MIME type
/// Note: Matching should be case insensitive.
pub const MIME_TYPE_G722: &str = "audio/G722";
/// MIME_TYPE_PCMU PCMU MIME type
/// Note: Matching should be case insensitive.
pub const MIME_TYPE_PCMU: &str = "audio/PCMU";
/// MIME_TYPE_PCMA PCMA MIME type
/// Note: Matching should be case insensitive.
pub const MIME_TYPE_PCMA: &str = "audio/PCMA";

#[derive(Default, Clone)]
pub(crate) struct MediaEngineHeaderExtension {
    pub(crate) uri: String,
    pub(crate) is_audio: bool,
    pub(crate) is_video: bool,
    // If set only Transceivers of this direction are allowed
    pub(crate) allowed_directions: Vec<RTCRtpTransceiverDirection>,
}

/// A MediaEngine defines the codecs supported by a PeerConnection, and the
/// configuration of those codecs. A MediaEngine must not be shared between
/// PeerConnections.
#[derive(Default)]
pub struct MediaEngine {
    // If we have attempted to negotiate a codec type yet.
    pub(crate) negotiated_video: AtomicBool,
    pub(crate) negotiated_audio: AtomicBool,

    pub(crate) video_codecs: Vec<RTCRtpCodecParameters>,
    pub(crate) audio_codecs: Vec<RTCRtpCodecParameters>,
    pub(crate) negotiated_video_codecs: Mutex<Vec<RTCRtpCodecParameters>>,
    pub(crate) negotiated_audio_codecs: Mutex<Vec<RTCRtpCodecParameters>>,

    pub(crate) header_extensions: Vec<MediaEngineHeaderExtension>,
    pub(crate) negotiated_header_extensions: Mutex<HashMap<isize, MediaEngineHeaderExtension>>,
}

impl MediaEngine {
    /// register_default_codecs registers the default codecs supported by Pion WebRTC.
    /// register_default_codecs is not safe for concurrent use.
    pub fn register_default_codecs(&mut self) -> Result<()> {
        // Default Audio Codecs
        for codec in vec![
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 111,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_G722.to_owned(),
                    clock_rate: 8000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 9,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_PCMU.to_owned(),
                    clock_rate: 8000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 0,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_PCMA.to_owned(),
                    clock_rate: 8000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 8,
                ..Default::default()
            },
        ] {
            self.register_codec(codec, RTPCodecType::Audio)?;
        }

        let video_rtcp_feedback = vec![
            RTCPFeedback {
                typ: "goog-remb".to_owned(),
                parameter: "".to_owned(),
            },
            RTCPFeedback {
                typ: "ccm".to_owned(),
                parameter: "fir".to_owned(),
            },
            RTCPFeedback {
                typ: "nack".to_owned(),
                parameter: "".to_owned(),
            },
            RTCPFeedback {
                typ: "nack".to_owned(),
                parameter: "pli".to_owned(),
            },
        ];
        for codec in vec![
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 96,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=96".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 97,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_VP9.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "profile-id=0".to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 98,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=98".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 99,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_VP9.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "profile-id=1".to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 100,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=100".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 101,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 102,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=102".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 121,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 127,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=127".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 120,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 125,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=125".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 107,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42e01f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 108,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=108".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 109,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 127,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=127".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 120,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640032"
                            .to_owned(),
                    rtcp_feedback: video_rtcp_feedback,
                },
                payload_type: 123,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=123".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 118,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/ulpfec".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 116,
                ..Default::default()
            },
        ] {
            self.register_codec(codec, RTPCodecType::Video)?;
        }

        Ok(())
    }

    /// add_codec will append codec if it not exists
    fn add_codec(codecs: &mut Vec<RTCRtpCodecParameters>, codec: RTCRtpCodecParameters) {
        for c in codecs.iter() {
            if c.capability.mime_type == codec.capability.mime_type
                && c.payload_type == codec.payload_type
            {
                return;
            }
        }
        codecs.push(codec);
    }

    /// register_codec adds codec to the MediaEngine
    /// These are the list of codecs supported by this PeerConnection.
    /// register_codec is not safe for concurrent use.
    pub fn register_codec(
        &mut self,
        mut codec: RTCRtpCodecParameters,
        typ: RTPCodecType,
    ) -> Result<()> {
        codec.stats_id = format!(
            "RTPCodec-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        match typ {
            RTPCodecType::Audio => {
                MediaEngine::add_codec(&mut self.audio_codecs, codec);
                Ok(())
            }
            RTPCodecType::Video => {
                MediaEngine::add_codec(&mut self.video_codecs, codec);
                Ok(())
            }
            _ => Err(Error::ErrUnknownType),
        }
    }

    /// register_header_extension adds a header extension to the MediaEngine
    /// To determine the negotiated value use `GetHeaderExtensionID` after signaling is complete
    pub async fn register_header_extension(
        &mut self,
        extension: RTCRtpHeaderExtensionCapability,
        typ: RTPCodecType,
        mut allowed_directions: Vec<RTCRtpTransceiverDirection>,
    ) -> Result<()> {
        if allowed_directions.is_empty() {
            allowed_directions = vec![
                RTCRtpTransceiverDirection::Recvonly,
                RTCRtpTransceiverDirection::Sendonly,
            ];
        }

        for direction in &allowed_directions {
            if *direction != RTCRtpTransceiverDirection::Recvonly
                && *direction != RTCRtpTransceiverDirection::Sendonly
            {
                return Err(Error::ErrRegisterHeaderExtensionInvalidDirection);
            }
        }

        let mut extension_index = None;
        for (i, ext) in self.header_extensions.iter().enumerate() {
            if extension.uri == ext.uri {
                extension_index = Some(i);
            }
        }

        if extension_index.is_none() {
            self.header_extensions
                .push(MediaEngineHeaderExtension::default());
            extension_index = Some(self.header_extensions.len() - 1);
        }

        if let Some(extension_index) = extension_index {
            if typ == RTPCodecType::Audio {
                self.header_extensions[extension_index].is_audio = true;
            } else if typ == RTPCodecType::Video {
                self.header_extensions[extension_index].is_video = true;
            }

            self.header_extensions[extension_index].uri = extension.uri;
            self.header_extensions[extension_index].allowed_directions = allowed_directions;
        }

        Ok(())
    }

    /// register_feedback adds feedback mechanism to already registered codecs.
    pub fn register_feedback(&mut self, feedback: RTCPFeedback, typ: RTPCodecType) {
        match typ {
            RTPCodecType::Video => {
                for v in &mut self.video_codecs {
                    v.capability.rtcp_feedback.push(feedback.clone());
                }
            }
            RTPCodecType::Audio => {
                for a in &mut self.audio_codecs {
                    a.capability.rtcp_feedback.push(feedback.clone());
                }
            }
            _ => {}
        }
    }

    /// get_header_extension_id returns the negotiated ID for a header extension.
    /// If the Header Extension isn't enabled ok will be false
    pub(crate) async fn get_header_extension_id(
        &self,
        extension: RTCRtpHeaderExtensionCapability,
    ) -> (isize, bool, bool) {
        let negotiated_header_extensions = self.negotiated_header_extensions.lock().await;
        if negotiated_header_extensions.is_empty() {
            return (0, false, false);
        }

        for (id, h) in &*negotiated_header_extensions {
            if extension.uri == h.uri {
                return (*id, h.is_audio, h.is_video);
            }
        }

        (0, false, false)
    }

    /// clone_to copies any user modifiable state of the MediaEngine
    /// all internal state is reset
    pub(crate) fn clone_to(&self) -> Self {
        MediaEngine {
            video_codecs: self.video_codecs.clone(),
            audio_codecs: self.audio_codecs.clone(),
            header_extensions: self.header_extensions.clone(),
            ..Default::default()
        }
    }

    pub(crate) async fn get_codec_by_payload(
        &self,
        payload_type: PayloadType,
    ) -> Result<(RTCRtpCodecParameters, RTPCodecType)> {
        {
            let negotiated_video_codecs = self.negotiated_video_codecs.lock().await;
            for codec in &*negotiated_video_codecs {
                if codec.payload_type == payload_type {
                    return Ok((codec.clone(), RTPCodecType::Video));
                }
            }
        }
        {
            let negotiated_audio_codecs = self.negotiated_audio_codecs.lock().await;
            for codec in &*negotiated_audio_codecs {
                if codec.payload_type == payload_type {
                    return Ok((codec.clone(), RTPCodecType::Audio));
                }
            }
        }

        Err(Error::ErrCodecNotFound)
    }

    /*TODO: func (m *MediaEngine) collectStats(collector *statsReportCollector) {

            statsLoop := func(codecs []RTPCodecParameters) {
                for _, codec := range codecs {
                    collector.Collecting()
                    stats := CodecStats{
                        Timestamp:   statsTimestampFrom(time.Now()),
                        Type:        StatsTypeCodec,
                        ID:          codec.statsID,
                        PayloadType: codec.PayloadType,
                        mime_type:    codec.mime_type,
                        clock_rate:   codec.clock_rate,
                        channels:    uint8(codec.channels),
                        sdpfmtp_line: codec.sdpfmtp_line,
                    }

                    collector.Collect(stats.ID, stats)
                }
            }

            statsLoop(m.videoCodecs)
            statsLoop(m.audioCodecs)
        }
    */

    /// Look up a codec and enable if it exists
    pub(crate) fn match_remote_codec(
        &self,
        remote_codec: &RTCRtpCodecParameters,
        typ: RTPCodecType,
        exact_matches: &[RTCRtpCodecParameters],
        partial_matches: &[RTCRtpCodecParameters],
    ) -> Result<CodecMatch> {
        let codecs = if typ == RTPCodecType::Audio {
            &self.audio_codecs
        } else {
            &self.video_codecs
        };

        let remote_fmtp = fmtp::parse(
            &remote_codec.capability.mime_type,
            remote_codec.capability.sdp_fmtp_line.as_str(),
        );
        if let Some(apt) = remote_fmtp.parameter("apt") {
            let payload_type = apt.parse::<u8>()?;

            let mut apt_match = CodecMatch::None;
            for codec in exact_matches {
                if codec.payload_type == payload_type {
                    apt_match = CodecMatch::Exact;
                    break;
                }
            }

            if apt_match == CodecMatch::None {
                for codec in partial_matches {
                    if codec.payload_type == payload_type {
                        apt_match = CodecMatch::Partial;
                        break;
                    }
                }
            }

            if apt_match == CodecMatch::None {
                return Ok(CodecMatch::None); // not an error, we just ignore this codec we don't support
            }

            // if apt's media codec is partial match, then apt codec must be partial match too
            let (_, mut match_type) = codec_parameters_fuzzy_search(remote_codec, codecs);
            if match_type == CodecMatch::Exact && apt_match == CodecMatch::Partial {
                match_type = CodecMatch::Partial;
            }
            return Ok(match_type);
        }

        let (_, match_type) = codec_parameters_fuzzy_search(remote_codec, codecs);
        Ok(match_type)
    }

    /// Look up a header extension and enable if it exists
    pub(crate) async fn update_header_extension(
        &self,
        id: isize,
        extension: &str,
        typ: RTPCodecType,
    ) -> Result<()> {
        for local_extension in &self.header_extensions {
            if local_extension.uri == extension {
                let mut negotiated_header_extensions =
                    self.negotiated_header_extensions.lock().await;
                if let Some(h) = negotiated_header_extensions.get_mut(&id) {
                    if local_extension.is_audio && typ == RTPCodecType::Audio {
                        h.is_audio = true;
                    } else if local_extension.is_video && typ == RTPCodecType::Video {
                        h.is_video = true;
                    }
                } else {
                    let h = MediaEngineHeaderExtension {
                        uri: extension.to_owned(),
                        is_audio: local_extension.is_audio && typ == RTPCodecType::Audio,
                        is_video: local_extension.is_video && typ == RTPCodecType::Video,
                        allowed_directions: local_extension.allowed_directions.clone(),
                    };
                    negotiated_header_extensions.insert(id, h);
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn push_codecs(&self, codecs: Vec<RTCRtpCodecParameters>, typ: RTPCodecType) {
        for codec in codecs {
            if typ == RTPCodecType::Audio {
                let mut negotiated_audio_codecs = self.negotiated_audio_codecs.lock().await;
                MediaEngine::add_codec(&mut negotiated_audio_codecs, codec);
            } else if typ == RTPCodecType::Video {
                let mut negotiated_video_codecs = self.negotiated_video_codecs.lock().await;
                MediaEngine::add_codec(&mut negotiated_video_codecs, codec);
            }
        }
    }

    /// Update the MediaEngine from a remote description
    pub(crate) async fn update_from_remote_description(
        &self,
        desc: &SessionDescription,
    ) -> Result<()> {
        for media in &desc.media_descriptions {
            let typ = if !self.negotiated_audio.load(Ordering::SeqCst)
                && media.media_name.media.to_lowercase() == "audio"
            {
                self.negotiated_audio.store(true, Ordering::SeqCst);
                RTPCodecType::Audio
            } else if !self.negotiated_video.load(Ordering::SeqCst)
                && media.media_name.media.to_lowercase() == "video"
            {
                self.negotiated_video.store(true, Ordering::SeqCst);
                RTPCodecType::Video
            } else {
                continue;
            };

            let codecs = codecs_from_media_description(media)?;

            let mut exact_matches = vec![]; //make([]RTPCodecParameters, 0, len(codecs))
            let mut partial_matches = vec![]; //make([]RTPCodecParameters, 0, len(codecs))

            for codec in codecs {
                let match_type =
                    self.match_remote_codec(&codec, typ, &exact_matches, &partial_matches)?;

                if match_type == CodecMatch::Exact {
                    exact_matches.push(codec);
                } else if match_type == CodecMatch::Partial {
                    partial_matches.push(codec);
                }
            }

            // use exact matches when they exist, otherwise fall back to partial
            if !exact_matches.is_empty() {
                self.push_codecs(exact_matches, typ).await;
            } else if !partial_matches.is_empty() {
                self.push_codecs(partial_matches, typ).await;
            } else {
                // no match, not negotiated
                continue;
            }

            let extensions = rtp_extensions_from_media_description(media)?;

            for (extension, id) in extensions {
                self.update_header_extension(id, &extension, typ).await?;
            }
        }

        Ok(())
    }

    pub(crate) async fn get_codecs_by_kind(&self, typ: RTPCodecType) -> Vec<RTCRtpCodecParameters> {
        if typ == RTPCodecType::Video {
            if self.negotiated_video.load(Ordering::SeqCst) {
                let negotiated_video_codecs = self.negotiated_video_codecs.lock().await;
                negotiated_video_codecs.clone()
            } else {
                self.video_codecs.clone()
            }
        } else if typ == RTPCodecType::Audio {
            if self.negotiated_audio.load(Ordering::SeqCst) {
                let negotiated_audio_codecs = self.negotiated_audio_codecs.lock().await;
                negotiated_audio_codecs.clone()
            } else {
                self.audio_codecs.clone()
            }
        } else {
            vec![]
        }
    }

    pub(crate) async fn get_rtp_parameters_by_kind(
        &self,
        typ: RTPCodecType,
        directions: &[RTCRtpTransceiverDirection],
    ) -> RTCRtpParameters {
        let mut header_extensions = vec![];

        if self.negotiated_video.load(Ordering::SeqCst) && typ == RTPCodecType::Video
            || self.negotiated_audio.load(Ordering::SeqCst) && typ == RTPCodecType::Audio
        {
            let negotiated_header_extensions = self.negotiated_header_extensions.lock().await;
            for (id, e) in &*negotiated_header_extensions {
                if have_rtp_transceiver_direction_intersection(&e.allowed_directions, directions)
                    && (e.is_audio && typ == RTPCodecType::Audio
                        || e.is_video && typ == RTPCodecType::Video)
                {
                    header_extensions.push(RTCRtpHeaderExtensionParameters {
                        id: *id,
                        uri: e.uri.clone(),
                    });
                }
            }
        } else {
            for (id, e) in self.header_extensions.iter().enumerate() {
                if have_rtp_transceiver_direction_intersection(&e.allowed_directions, directions)
                    && (e.is_audio && typ == RTPCodecType::Audio
                        || e.is_video && typ == RTPCodecType::Video)
                {
                    header_extensions.push(RTCRtpHeaderExtensionParameters {
                        id: id as isize + 1,
                        uri: e.uri.clone(),
                    })
                }
            }
        }

        RTCRtpParameters {
            header_extensions,
            codecs: self.get_codecs_by_kind(typ).await,
        }
    }

    pub(crate) async fn get_rtp_parameters_by_payload_type(
        &self,
        payload_type: PayloadType,
    ) -> Result<RTCRtpParameters> {
        let (codec, typ) = self.get_codec_by_payload(payload_type).await?;

        let mut header_extensions = vec![];
        {
            let negotiated_header_extensions = self.negotiated_header_extensions.lock().await;
            for (id, e) in &*negotiated_header_extensions {
                if e.is_audio && typ == RTPCodecType::Audio
                    || e.is_video && typ == RTPCodecType::Video
                {
                    header_extensions.push(RTCRtpHeaderExtensionParameters {
                        uri: e.uri.clone(),
                        id: *id,
                    });
                }
            }
        }

        Ok(RTCRtpParameters {
            header_extensions,
            codecs: vec![codec],
        })
    }
}
