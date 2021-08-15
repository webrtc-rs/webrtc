use crate::media::rtp::rtp_codec::{
    RTPCodecCapability, RTPCodecParameters, RTPCodecType, RTPHeaderExtensionParameter,
    RTPParameters,
};
use crate::media::rtp::rtp_transceiver_direction::{
    have_rtp_transceiver_direction_intersection, RTPTransceiverDirection,
};

use crate::error::Error;
use crate::media::rtp::RTCPFeedback;
use anyhow::Result;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

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
    pub(crate) allowed_directions: Vec<RTPTransceiverDirection>,
}

/// A MediaEngine defines the codecs supported by a PeerConnection, and the
/// configuration of those codecs. A MediaEngine must not be shared between
/// PeerConnections.
#[derive(Default, Clone)]
pub struct MediaEngine {
    // If we have attempted to negotiate a codec type yet.
    pub(crate) negotiated_video: bool,
    pub(crate) negotiated_audio: bool,

    pub(crate) video_codecs: Vec<RTPCodecParameters>,
    pub(crate) audio_codecs: Vec<RTPCodecParameters>,
    pub(crate) negotiated_video_codecs: Vec<RTPCodecParameters>,
    pub(crate) negotiated_audio_codecs: Vec<RTPCodecParameters>,

    pub(crate) header_extensions: Vec<MediaEngineHeaderExtension>,
    pub(crate) negotiated_header_extensions: HashMap<isize, MediaEngineHeaderExtension>,
}

impl MediaEngine {
    /// register_default_codecs registers the default codecs supported by Pion WebRTC.
    /// register_default_codecs is not safe for concurrent use.
    pub fn register_default_codecs(&mut self) -> Result<()> {
        // Default Audio Codecs
        for codec in vec![
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 111,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_G722.to_owned(),
                    clock_rate: 8000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 9,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_PCMU.to_owned(),
                    clock_rate: 8000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 0,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
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
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 96,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=96".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 97,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_VP9.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "profile-id=0".to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 98,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=98".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 99,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_VP9.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "profile-id=1".to_owned(),
                    rtcp_feedback: video_rtcp_feedback.clone(),
                },
                payload_type: 100,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=100".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 101,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
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
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=102".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 121,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
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
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=127".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 120,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
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
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=125".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 107,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
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
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=108".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 109,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
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
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=127".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 120,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
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
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=123".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 118,
                ..Default::default()
            },
            RTPCodecParameters {
                capability: RTPCodecCapability {
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
    fn add_codec(codecs: &mut Vec<RTPCodecParameters>, codec: RTPCodecParameters) {
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
        mut codec: RTPCodecParameters,
        typ: RTPCodecType,
    ) -> Result<()> {
        codec.stats_id = format!(
            "RTPCodec-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
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
            _ => Err(Error::ErrUnknownType.into()),
        }
    }

    /*
    // RegisterHeaderExtension adds a header extension to the MediaEngine
    // To determine the negotiated value use `GetHeaderExtensionID` after signaling is complete
    func (m *MediaEngine) RegisterHeaderExtension(extension RTPHeaderExtensionCapability, typ RTPCodecType, allowedDirections ...RTPTransceiverDirection) error {
        if m.negotiated_header_extensions == nil {
            m.negotiated_header_extensions = map[int]mediaEngineHeaderExtension{}
        }

        if len(allowedDirections) == 0 {
            allowedDirections = []RTPTransceiverDirection{RTPTransceiverDirectionRecvonly, RTPTransceiverDirectionSendonly}
        }

        for _, direction := range allowedDirections {
            if direction != RTPTransceiverDirectionRecvonly && direction != RTPTransceiverDirectionSendonly {
                return ErrRegisterHeaderExtensionInvalidDirection
            }
        }

        extensionIndex := -1
        for i := range m.header_extensions {
            if extension.URI == m.header_extensions[i].uri {
                extensionIndex = i
            }
        }

        if extensionIndex == -1 {
            m.header_extensions = append(m.header_extensions, mediaEngineHeaderExtension{})
            extensionIndex = len(m.header_extensions) - 1
        }

        if typ == RTPCodecTypeAudio {
            m.header_extensions[extensionIndex].is_audio = true
        } else if typ == RTPCodecTypeVideo {
            m.header_extensions[extensionIndex].is_video = true
        }

        m.header_extensions[extensionIndex].uri = extension.URI
        m.header_extensions[extensionIndex].allowedDirections = allowedDirections

        return nil
    }

    // RegisterFeedback adds feedback mechanism to already registered codecs.
    func (m *MediaEngine) RegisterFeedback(feedback rtcpfeedback, typ RTPCodecType) {
        switch typ {
        case RTPCodecTypeVideo:
            for i, v := range m.videoCodecs {
                v.rtcpfeedback = append(v.rtcpfeedback, feedback)
                m.videoCodecs[i] = v
            }
        case RTPCodecTypeAudio:
            for i, v := range m.audioCodecs {
                v.rtcpfeedback = append(v.rtcpfeedback, feedback)
                m.audioCodecs[i] = v
            }
        }
    }

    // getHeaderExtensionID returns the negotiated ID for a header extension.
    // If the Header Extension isn't enabled ok will be false
    func (m *MediaEngine) getHeaderExtensionID(extension RTPHeaderExtensionCapability) (val int, audioNegotiated, videoNegotiated bool) {
        if m.negotiated_header_extensions == nil {
            return 0, false, false
        }

        for id, h := range m.negotiated_header_extensions {
            if extension.URI == h.uri {
                return id, h.is_audio, h.is_video
            }
        }

        return
    }*/

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
    /*
    func (m *MediaEngine) getCodecByPayload(payloadType PayloadType) (RTPCodecParameters, RTPCodecType, error) {
        for _, codec := range m.negotiatedVideoCodecs {
            if codec.PayloadType == payloadType {
                return codec, RTPCodecTypeVideo, nil
            }
        }
        for _, codec := range m.negotiatedAudioCodecs {
            if codec.PayloadType == payloadType {
                return codec, RTPCodecTypeAudio, nil
            }
        }

        return RTPCodecParameters{}, 0, ErrCodecNotFound
    }

    func (m *MediaEngine) collectStats(collector *statsReportCollector) {
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

    // Look up a codec and enable if it exists
    func (m *MediaEngine) matchRemoteCodec(remoteCodec RTPCodecParameters, typ RTPCodecType, exactMatches, partialMatches []RTPCodecParameters) (codecMatchType, error) {
        codecs := m.videoCodecs
        if typ == RTPCodecTypeAudio {
            codecs = m.audioCodecs
        }

        remoteFmtp := parse_fmtp(remoteCodec.RTPCodecCapability.sdpfmtp_line)
        if apt, hasApt := remoteFmtp["apt"]; hasApt {
            payloadType, err := strconv.Atoi(apt)
            if err != nil {
                return codecMatchNone, err
            }

            aptMatch := codecMatchNone
            for _, codec := range exactMatches {
                if codec.PayloadType == PayloadType(payloadType) {
                    aptMatch = codecMatchExact
                    break
                }
            }

            if aptMatch == codecMatchNone {
                for _, codec := range partialMatches {
                    if codec.PayloadType == PayloadType(payloadType) {
                        aptMatch = codecMatchPartial
                        break
                    }
                }
            }

            if aptMatch == codecMatchNone {
                return codecMatchNone, nil // not an error, we just ignore this codec we don't support
            }

            // if apt's media codec is partial match, then apt codec must be partial match too
            _, matchType := codec_parameters_fuzzy_search(remoteCodec, codecs)
            if matchType == codecMatchExact && aptMatch == codecMatchPartial {
                matchType = codecMatchPartial
            }
            return matchType, nil
        }

        _, matchType := codec_parameters_fuzzy_search(remoteCodec, codecs)
        return matchType, nil
    }

    // Look up a header extension and enable if it exists
    func (m *MediaEngine) updateHeaderExtension(id int, extension string, typ RTPCodecType) error {
        if m.negotiated_header_extensions == nil {
            return nil
        }

        for _, localExtension := range m.header_extensions {
            if localExtension.uri == extension {
                h := mediaEngineHeaderExtension{uri: extension, allowedDirections: localExtension.allowedDirections}
                if existingValue, ok := m.negotiated_header_extensions[id]; ok {
                    h = existingValue
                }

                switch {
                case localExtension.is_audio && typ == RTPCodecTypeAudio:
                    h.is_audio = true
                case localExtension.is_video && typ == RTPCodecTypeVideo:
                    h.is_video = true
                }

                m.negotiated_header_extensions[id] = h
            }
        }
        return nil
    }*/

    pub(crate) fn push_codecs(&mut self, codecs: Vec<RTPCodecParameters>, typ: RTPCodecType) {
        for codec in codecs {
            if typ == RTPCodecType::Audio {
                MediaEngine::add_codec(&mut self.negotiated_audio_codecs, codec);
            } else if typ == RTPCodecType::Video {
                MediaEngine::add_codec(&mut self.negotiated_video_codecs, codec);
            }
        }
    }
    /*
    // Update the MediaEngine from a remote description
    func (m *MediaEngine) updateFromRemoteDescription(desc sdp.SessionDescription) error {
        for _, media := range desc.MediaDescriptions {
            var typ RTPCodecType
            switch {
            case !m.negotiated_audio && strings.EqualFold(media.MediaName.Media, "audio"):
                m.negotiated_audio = true
                typ = RTPCodecTypeAudio
            case !m.negotiated_video && strings.EqualFold(media.MediaName.Media, "video"):
                m.negotiated_video = true
                typ = RTPCodecTypeVideo
            default:
                continue
            }

            codecs, err := codecs_from_media_description(media)
            if err != nil {
                return err
            }

            exactMatches := make([]RTPCodecParameters, 0, len(codecs))
            partialMatches := make([]RTPCodecParameters, 0, len(codecs))

            for _, codec := range codecs {
                matchType, mErr := m.matchRemoteCodec(codec, typ, exactMatches, partialMatches)
                if mErr != nil {
                    return mErr
                }

                if matchType == codecMatchExact {
                    exactMatches = append(exactMatches, codec)
                } else if matchType == codecMatchPartial {
                    partialMatches = append(partialMatches, codec)
                }
            }

            // use exact matches when they exist, otherwise fall back to partial
            switch {
            case len(exactMatches) > 0:
                m.pushCodecs(exactMatches, typ)
            case len(partialMatches) > 0:
                m.pushCodecs(partialMatches, typ)
            default:
                // no match, not negotiated
                continue
            }

            extensions, err := rtp_extensions_from_media_description(media)
            if err != nil {
                return err
            }

            for extension, id := range extensions {
                if err = m.updateHeaderExtension(id, extension, typ); err != nil {
                    return err
                }
            }
        }
        return nil
    }
    */

    pub(crate) fn get_codecs_by_kind(&self, typ: RTPCodecType) -> Vec<RTPCodecParameters> {
        if typ == RTPCodecType::Video {
            if self.negotiated_video {
                self.negotiated_video_codecs.clone()
            } else {
                self.video_codecs.clone()
            }
        } else if typ == RTPCodecType::Audio {
            if self.negotiated_audio {
                self.negotiated_audio_codecs.clone()
            } else {
                self.audio_codecs.clone()
            }
        } else {
            vec![]
        }
    }

    pub(crate) fn get_rtp_parameters_by_kind(
        &self,
        typ: RTPCodecType,
        directions: &[RTPTransceiverDirection],
    ) -> RTPParameters {
        let mut header_extensions = vec![];

        if self.negotiated_video && typ == RTPCodecType::Video
            || self.negotiated_audio && typ == RTPCodecType::Audio
        {
            for (id, e) in &self.negotiated_header_extensions {
                if have_rtp_transceiver_direction_intersection(&e.allowed_directions, directions)
                    && (e.is_audio && typ == RTPCodecType::Audio
                        || e.is_video && typ == RTPCodecType::Video)
                {
                    header_extensions.push(RTPHeaderExtensionParameter {
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
                    header_extensions.push(RTPHeaderExtensionParameter {
                        id: id as isize + 1,
                        uri: e.uri.clone(),
                    })
                }
            }
        }

        RTPParameters {
            header_extensions,
            codecs: self.get_codecs_by_kind(typ),
        }
    }
    /*
    func (m *MediaEngine) getRTPParametersByPayloadType(payloadType PayloadType) (RTPParameters, error) {
        codec, typ, err := m.getCodecByPayload(payloadType)
        if err != nil {
            return RTPParameters{}, err
        }

        header_extensions := make([]RTPHeaderExtensionParameter, 0)
        for id, e := range m.negotiated_header_extensions {
            if e.is_audio && typ == RTPCodecTypeAudio || e.is_video && typ == RTPCodecTypeVideo {
                header_extensions = append(header_extensions, RTPHeaderExtensionParameter{ID: id, URI: e.uri})
            }
        }

        return RTPParameters{
            header_extensions: header_extensions,
            Codecs:           []RTPCodecParameters{codec},
        }, nil
    }

    */
}
