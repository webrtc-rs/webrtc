use std::collections::HashMap;

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

pub(crate) struct MediaEngineHeaderExtension {
    uri: String,
    is_audio: bool,
    is_video: bool,
    // If set only Transceivers of this direction are allowed
    //TODO: allowedDirections []RTPTransceiverDirection
}

/// A MediaEngine defines the codecs supported by a PeerConnection, and the
/// configuration of those codecs. A MediaEngine must not be shared between
/// PeerConnections.
#[derive(Default)]
pub struct MediaEngine {
    // If we have attempted to negotiate a codec type yet.
    negotiated_video: bool,
    negotiated_audio: bool,

    //TODO: videoCodecs, audioCodecs                     []RTPCodecParameters
    //TODO: negotiatedVideoCodecs, negotiatedAudioCodecs []RTPCodecParameters
    header_extensions: Vec<MediaEngineHeaderExtension>,
    negotiated_header_extensions: HashMap<usize, MediaEngineHeaderExtension>,
}

/*
// RegisterDefaultCodecs registers the default codecs supported by Pion WebRTC.
// RegisterDefaultCodecs is not safe for concurrent use.
func (m *MediaEngine) RegisterDefaultCodecs() error {
    // Default Pion Audio Codecs
    for _, codec := range []RTPCodecParameters{
        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_OPUS, 48000, 2, "minptime=10;useinbandfec=1", nil},
            PayloadType:        111,
        },
        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_G722, 8000, 0, "", nil},
            PayloadType:        9,
        },
        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_PCMU, 8000, 0, "", nil},
            PayloadType:        0,
        },
        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_PCMA, 8000, 0, "", nil},
            PayloadType:        8,
        },
    } {
        if err := m.RegisterCodec(codec, RTPCodecTypeAudio); err != nil {
            return err
        }
    }

    videoRTCPFeedback := []rtcpfeedback{{"goog-remb", ""}, {"ccm", "fir"}, {"nack", ""}, {"nack", "pli"}}
    for _, codec := range []RTPCodecParameters{
        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_VP8, 90000, 0, "", videoRTCPFeedback},
            PayloadType:        96,
        },
        {
            RTPCodecCapability: RTPCodecCapability{"video/rtx", 90000, 0, "apt=96", nil},
            PayloadType:        97,
        },

        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_VP9, 90000, 0, "profile-id=0", videoRTCPFeedback},
            PayloadType:        98,
        },
        {
            RTPCodecCapability: RTPCodecCapability{"video/rtx", 90000, 0, "apt=98", nil},
            PayloadType:        99,
        },

        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_VP9, 90000, 0, "profile-id=1", videoRTCPFeedback},
            PayloadType:        100,
        },
        {
            RTPCodecCapability: RTPCodecCapability{"video/rtx", 90000, 0, "apt=100", nil},
            PayloadType:        101,
        },

        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_H264, 90000, 0, "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f", videoRTCPFeedback},
            PayloadType:        102,
        },
        {
            RTPCodecCapability: RTPCodecCapability{"video/rtx", 90000, 0, "apt=102", nil},
            PayloadType:        121,
        },

        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_H264, 90000, 0, "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f", videoRTCPFeedback},
            PayloadType:        127,
        },
        {
            RTPCodecCapability: RTPCodecCapability{"video/rtx", 90000, 0, "apt=127", nil},
            PayloadType:        120,
        },

        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_H264, 90000, 0, "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f", videoRTCPFeedback},
            PayloadType:        125,
        },
        {
            RTPCodecCapability: RTPCodecCapability{"video/rtx", 90000, 0, "apt=125", nil},
            PayloadType:        107,
        },

        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_H264, 90000, 0, "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42e01f", videoRTCPFeedback},
            PayloadType:        108,
        },
        {
            RTPCodecCapability: RTPCodecCapability{"video/rtx", 90000, 0, "apt=108", nil},
            PayloadType:        109,
        },

        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_H264, 90000, 0, "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f", videoRTCPFeedback},
            PayloadType:        127,
        },
        {
            RTPCodecCapability: RTPCodecCapability{"video/rtx", 90000, 0, "apt=127", nil},
            PayloadType:        120,
        },

        {
            RTPCodecCapability: RTPCodecCapability{MIME_TYPE_H264, 90000, 0, "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640032", videoRTCPFeedback},
            PayloadType:        123,
        },
        {
            RTPCodecCapability: RTPCodecCapability{"video/rtx", 90000, 0, "apt=123", nil},
            PayloadType:        118,
        },

        {
            RTPCodecCapability: RTPCodecCapability{"video/ulpfec", 90000, 0, "", nil},
            PayloadType:        116,
        },
    } {
        if err := m.RegisterCodec(codec, RTPCodecTypeVideo); err != nil {
            return err
        }
    }

    return nil
}

// addCodec will append codec if it not exists
func (m *MediaEngine) addCodec(codecs []RTPCodecParameters, codec RTPCodecParameters) []RTPCodecParameters {
    for _, c := range codecs {
        if c.mime_type == codec.mime_type && c.PayloadType == codec.PayloadType {
            return codecs
        }
    }
    return append(codecs, codec)
}

// RegisterCodec adds codec to the MediaEngine
// These are the list of codecs supported by this PeerConnection.
// RegisterCodec is not safe for concurrent use.
func (m *MediaEngine) RegisterCodec(codec RTPCodecParameters, typ RTPCodecType) error {
    codec.statsID = fmt.Sprintf("RTPCodec-%d", time.Now().UnixNano())
    switch typ {
    case RTPCodecTypeAudio:
        m.audioCodecs = m.addCodec(m.audioCodecs, codec)
    case RTPCodecTypeVideo:
        m.videoCodecs = m.addCodec(m.videoCodecs, codec)
    default:
        return ErrUnknownType
    }
    return nil
}

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
}

// copy copies any user modifiable state of the MediaEngine
// all internal state is reset
func (m *MediaEngine) copy() *MediaEngine {
    cloned := &MediaEngine{
        videoCodecs:      append([]RTPCodecParameters{}, m.videoCodecs...),
        audioCodecs:      append([]RTPCodecParameters{}, m.audioCodecs...),
        header_extensions: append([]mediaEngineHeaderExtension{}, m.header_extensions...),
    }
    if len(m.header_extensions) > 0 {
        cloned.negotiated_header_extensions = map[int]mediaEngineHeaderExtension{}
    }
    return cloned
}

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
}

func (m *MediaEngine) pushCodecs(codecs []RTPCodecParameters, typ RTPCodecType) {
    for _, codec := range codecs {
        if typ == RTPCodecTypeAudio {
            m.negotiatedAudioCodecs = m.addCodec(m.negotiatedAudioCodecs, codec)
        } else if typ == RTPCodecTypeVideo {
            m.negotiatedVideoCodecs = m.addCodec(m.negotiatedVideoCodecs, codec)
        }
    }
}

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

        codecs, err := codecsFromMediaDescription(media)
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

        extensions, err := rtpExtensionsFromMediaDescription(media)
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

func (m *MediaEngine) getCodecsByKind(typ RTPCodecType) []RTPCodecParameters {
    if typ == RTPCodecTypeVideo {
        if m.negotiated_video {
            return m.negotiatedVideoCodecs
        }

        return m.videoCodecs
    } else if typ == RTPCodecTypeAudio {
        if m.negotiated_audio {
            return m.negotiatedAudioCodecs
        }

        return m.audioCodecs
    }

    return nil
}

func (m *MediaEngine) getRTPParametersByKind(typ RTPCodecType, directions []RTPTransceiverDirection) RTPParameters {
    header_extensions := make([]RTPHeaderExtensionParameter, 0)

    if m.negotiated_video && typ == RTPCodecTypeVideo ||
        m.negotiated_audio && typ == RTPCodecTypeAudio {
        for id, e := range m.negotiated_header_extensions {
            if haveRTPTransceiverDirectionIntersection(e.allowedDirections, directions) && (e.is_audio && typ == RTPCodecTypeAudio || e.is_video && typ == RTPCodecTypeVideo) {
                header_extensions = append(header_extensions, RTPHeaderExtensionParameter{ID: id, URI: e.uri})
            }
        }
    } else {
        for id, e := range m.header_extensions {
            if haveRTPTransceiverDirectionIntersection(e.allowedDirections, directions) && (e.is_audio && typ == RTPCodecTypeAudio || e.is_video && typ == RTPCodecTypeVideo) {
                header_extensions = append(header_extensions, RTPHeaderExtensionParameter{ID: id + 1, URI: e.uri})
            }
        }
    }

    return RTPParameters{
        header_extensions: header_extensions,
        Codecs:           m.getCodecsByKind(typ),
    }
}

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

func payloaderForCodec(codec RTPCodecCapability) (rtp.Payloader, error) {
    switch strings.ToLower(codec.mime_type) {
    case strings.ToLower(MIME_TYPE_H264):
        return &codecs.H264Payloader{}, nil
    case strings.ToLower(MIME_TYPE_OPUS):
        return &codecs.OpusPayloader{}, nil
    case strings.ToLower(MIME_TYPE_VP8):
        return &codecs.VP8Payloader{}, nil
    case strings.ToLower(MIME_TYPE_VP9):
        return &codecs.VP9Payloader{}, nil
    case strings.ToLower(MIME_TYPE_G722):
        return &codecs.G722Payloader{}, nil
    case strings.ToLower(MIME_TYPE_PCMU), strings.ToLower(MIME_TYPE_PCMA):
        return &codecs.G711Payloader{}, nil
    default:
        return nil, ErrNoPayloaderForCodec
    }
}
*/
