use super::*;
use crate::api::media_engine::MIME_TYPE_OPUS;
use crate::api::APIBuilder;
use crate::peer::configuration::Configuration;
use regex::Regex;
use std::io::Cursor;

#[tokio::test]
async fn test_opus_case() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let mut pc = api.new_peer_connection(Configuration::default()).await?;
    pc.add_transceiver_from_kind(RTPCodecType::Audio, &[])
        .await?;

    let offer = pc.create_offer(None).await?;

    let re = Regex::new(r"(?m)^a=rtpmap:\d+ opus/48000/2")?;
    assert!(re.is_match(offer.serde.sdp.as_str()));

    pc.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_video_case() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let mut pc = api.new_peer_connection(Configuration::default()).await?;
    pc.add_transceiver_from_kind(RTPCodecType::Video, &[])
        .await?;

    let offer = pc.create_offer(None).await?;

    let re = Regex::new(r"(?m)^a=rtpmap:\d+ H264/90000")?;
    assert!(re.is_match(offer.serde.sdp.as_str()));
    let re = Regex::new(r"(?m)^a=rtpmap:\d+ VP8/90000")?;
    assert!(re.is_match(offer.serde.sdp.as_str()));
    let re = Regex::new(r"(?m)^a=rtpmap:\d+ VP9/90000")?;
    assert!(re.is_match(offer.serde.sdp.as_str()));

    pc.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_media_engine_remote_description() -> Result<()> {
    let must_parse = |raw: &str| -> Result<sdp::session_description::SessionDescription> {
        let mut reader = Cursor::new(raw.as_bytes());
        sdp::session_description::SessionDescription::unmarshal(&mut reader)
    };

    //"No Media"
    {
        const NO_MEDIA: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
";
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        m.update_from_remote_description(&must_parse(NO_MEDIA)?)
            .await?;

        assert!(!m.negotiated_video.load(Ordering::SeqCst));
        assert!(!m.negotiated_audio.load(Ordering::SeqCst));
    }

    //"Enable Opus"
    {
        const OPUS_SAME_PAYLOAD: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=audio 9 UDP/TLS/RTP/SAVPF 111
a=rtpmap:111 opus/48000/2
a=fmtp:111 minptime=10; useinbandfec=1
";

        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        m.update_from_remote_description(&must_parse(OPUS_SAME_PAYLOAD)?)
            .await?;

        assert!(!m.negotiated_video.load(Ordering::SeqCst));
        assert!(m.negotiated_audio.load(Ordering::SeqCst));

        let (opus_codec, _) = m.get_codec_by_payload(111).await?;
        assert_eq!(opus_codec.capability.mime_type, MIME_TYPE_OPUS);
    }

    //"Change Payload Type"
    {
        const OPUS_SAME_PAYLOAD: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=audio 9 UDP/TLS/RTP/SAVPF 112
a=rtpmap:112 opus/48000/2
a=fmtp:112 minptime=10; useinbandfec=1
";

        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        m.update_from_remote_description(&must_parse(OPUS_SAME_PAYLOAD)?)
            .await?;

        assert!(!m.negotiated_video.load(Ordering::SeqCst));
        assert!(m.negotiated_audio.load(Ordering::SeqCst));

        let result = m.get_codec_by_payload(111).await;
        assert!(result.is_err());

        let (opus_codec, _) = m.get_codec_by_payload(112).await?;
        assert_eq!(opus_codec.capability.mime_type, MIME_TYPE_OPUS);
    }

    //"Case Insensitive"
    {
        const OPUS_UPCASE: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=audio 9 UDP/TLS/RTP/SAVPF 111
a=rtpmap:111 OPUS/48000/2
a=fmtp:111 minptime=10; useinbandfec=1
";

        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        m.update_from_remote_description(&must_parse(OPUS_UPCASE)?)
            .await?;

        assert!(!m.negotiated_video.load(Ordering::SeqCst));
        assert!(m.negotiated_audio.load(Ordering::SeqCst));

        let (opus_codec, _) = m.get_codec_by_payload(111).await?;
        assert_eq!(opus_codec.capability.mime_type, "audio/OPUS");
    }

    //"Handle different fmtp"
    {
        const OPUS_NO_FMTP: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=audio 9 UDP/TLS/RTP/SAVPF 111
a=rtpmap:111 opus/48000/2
";

        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        m.update_from_remote_description(&must_parse(OPUS_NO_FMTP)?)
            .await?;

        assert!(!m.negotiated_video.load(Ordering::SeqCst));
        assert!(m.negotiated_audio.load(Ordering::SeqCst));

        let (opus_codec, _) = m.get_codec_by_payload(111).await?;
        assert_eq!(opus_codec.capability.mime_type, MIME_TYPE_OPUS);
    }

    //"Header Extensions"
    {
        const HEADER_EXTENSIONS: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=audio 9 UDP/TLS/RTP/SAVPF 111
a=extmap:7 urn:ietf:params:rtp-hdrext:sdes:mid
a=extmap:5 urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id
a=rtpmap:111 opus/48000/2
";

        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        for extension in [
            "urn:ietf:params:rtp-hdrext:sdes:mid",
            "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id",
        ] {
            m.register_header_extension(
                RTPHeaderExtensionCapability {
                    uri: extension.to_owned(),
                },
                RTPCodecType::Audio,
                vec![],
            )
            .await?;
        }

        m.update_from_remote_description(&must_parse(HEADER_EXTENSIONS)?)
            .await?;

        assert!(!m.negotiated_video.load(Ordering::SeqCst));
        assert!(m.negotiated_audio.load(Ordering::SeqCst));

        let (abs_id, abs_audio_enabled, abs_video_enabled) = m
            .get_header_extension_id(RTPHeaderExtensionCapability {
                uri: sdp::extmap::ABS_SEND_TIME_URI.to_owned(),
            })
            .await;
        assert_eq!(abs_id, 0);
        assert!(!abs_audio_enabled);
        assert!(!abs_video_enabled);

        let (mid_id, mid_audio_enabled, mid_video_enabled) = m
            .get_header_extension_id(RTPHeaderExtensionCapability {
                uri: sdp::extmap::SDES_MID_URI.to_owned(),
            })
            .await;
        assert_eq!(mid_id, 7);
        assert!(mid_audio_enabled);
        assert!(!mid_video_enabled);
    }

    //"Prefers exact codec matches"
    {
        const PROFILE_LEVELS: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 60323 UDP/TLS/RTP/SAVPF 96 98
a=rtpmap:96 H264/90000
a=fmtp:96 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640c1f
a=rtpmap:98 H264/90000
a=fmtp:98 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f
";
        let mut m = MediaEngine::default();
        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                            .to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 127,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;
        m.update_from_remote_description(&must_parse(PROFILE_LEVELS)?)
            .await?;

        assert!(m.negotiated_video.load(Ordering::SeqCst));
        assert!(!m.negotiated_audio.load(Ordering::SeqCst));

        let (supported_h264, _) = m.get_codec_by_payload(98).await?;
        assert_eq!(supported_h264.capability.mime_type, MIME_TYPE_H264);

        assert!(m.get_codec_by_payload(96).await.is_err());
    }

    //"Does not match when fmtpline is set and does not match"
    {
        const PROFILE_LEVELS: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 60323 UDP/TLS/RTP/SAVPF 96 98
a=rtpmap:96 H264/90000
a=fmtp:96 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640c1f
";
        let mut m = MediaEngine::default();
        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line:
                        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                            .to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 127,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;
        assert!(m
            .update_from_remote_description(&must_parse(PROFILE_LEVELS)?)
            .await
            .is_err());

        assert!(m.get_codec_by_payload(96).await.is_err());
    }

    //"Matches when fmtpline is not set in offer, but exists in mediaengine"
    {
        const PROFILE_LEVELS: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 60323 UDP/TLS/RTP/SAVPF 96
a=rtpmap:96 VP9/90000
";
        let mut m = MediaEngine::default();
        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_VP9.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "profile-id=0".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 98,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        m.update_from_remote_description(&must_parse(PROFILE_LEVELS)?)
            .await?;

        assert!(m.negotiated_video.load(Ordering::SeqCst));

        m.get_codec_by_payload(96).await?;
    }

    //"Matches when fmtpline exists in neither"
    {
        const PROFILE_LEVELS: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 60323 UDP/TLS/RTP/SAVPF 96
a=rtpmap:96 VP8/90000
";
        let mut m = MediaEngine::default();
        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        m.update_from_remote_description(&must_parse(PROFILE_LEVELS)?)
            .await?;

        assert!(m.negotiated_video.load(Ordering::SeqCst));

        m.get_codec_by_payload(96).await?;
    }

    //"Matches when rtx apt for exact match codec"
    {
        const PROFILE_LEVELS: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 60323 UDP/TLS/RTP/SAVPF 94 96 97
a=rtpmap:94 VP8/90000
a=rtpmap:96 VP9/90000
a=fmtp:96 profile-id=2
a=rtpmap:97 rtx/90000
a=fmtp:97 apt=96
";
        let mut m = MediaEngine::default();
        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 94,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_VP9.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "profile-id=2".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=96".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 97,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        m.update_from_remote_description(&must_parse(PROFILE_LEVELS)?)
            .await?;

        assert!(m.negotiated_video.load(Ordering::SeqCst));

        m.get_codec_by_payload(97).await?;
    }

    //"Matches when rtx apt for partial match codec"
    {
        const PROFILE_LEVELS: &str = "v=0
o=- 4596489990601351948 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 60323 UDP/TLS/RTP/SAVPF 94 96 97
a=rtpmap:94 VP8/90000
a=rtpmap:96 VP9/90000
a=fmtp:96 profile-id=2
a=rtpmap:97 rtx/90000
a=fmtp:97 apt=96
";
        let mut m = MediaEngine::default();
        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 94,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_VP9.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "profile-id=1".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        m.register_codec(
            RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: "video/rtx".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "apt=96".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 97,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        m.update_from_remote_description(&must_parse(PROFILE_LEVELS)?)
            .await?;

        assert!(m.negotiated_video.load(Ordering::SeqCst));

        if let Err(err) = m.get_codec_by_payload(97).await {
            assert!(Error::ErrCodecNotFound.equal(&err));
        } else {
            assert!(false);
        }
    }

    Ok(())
}

/*
func TestMediaEngineHeaderExtensionDirection()->Result<()> {
    report := test.CheckRoutines(t)
    defer report()

    registerCodec := func(m *MediaEngine) {
        assert.NoError(t, m.RegisterCodec(
            RTPCodecParameters{
                RTPCodecCapability: RTPCodecCapability{MimeTypeOpus, 48000, 0, "", nil},
                PayloadType:        111,
            }, RTPCodecTypeAudio))
    }

    t.Run("No Direction", func()->Result<()> {
        m := &MediaEngine{}
        registerCodec(m)
        assert.NoError(t, m.RegisterHeaderExtension(RTPHeaderExtensionCapability{"pion-header-test"}, RTPCodecTypeAudio))

        params := m.getRTPParametersByKind(RTPCodecTypeAudio, []RTPTransceiverDirection{RTPTransceiverDirectionRecvonly})

        assert.Equal(t, 1, len(params.HeaderExtensions))
    })

    t.Run("Same Direction", func()->Result<()> {
        m := &MediaEngine{}
        registerCodec(m)
        assert.NoError(t, m.RegisterHeaderExtension(RTPHeaderExtensionCapability{"pion-header-test"}, RTPCodecTypeAudio, RTPTransceiverDirectionRecvonly))

        params := m.getRTPParametersByKind(RTPCodecTypeAudio, []RTPTransceiverDirection{RTPTransceiverDirectionRecvonly})

        assert.Equal(t, 1, len(params.HeaderExtensions))
    })

    t.Run("Different Direction", func()->Result<()> {
        m := &MediaEngine{}
        registerCodec(m)
        assert.NoError(t, m.RegisterHeaderExtension(RTPHeaderExtensionCapability{"pion-header-test"}, RTPCodecTypeAudio, RTPTransceiverDirectionSendonly))

        params := m.getRTPParametersByKind(RTPCodecTypeAudio, []RTPTransceiverDirection{RTPTransceiverDirectionRecvonly})

        assert.Equal(t, 0, len(params.HeaderExtensions))
    })

    t.Run("Invalid Direction", func()->Result<()> {
        m := &MediaEngine{}
        registerCodec(m)

        assert.Error(t, m.RegisterHeaderExtension(RTPHeaderExtensionCapability{"pion-header-test"}, RTPCodecTypeAudio, RTPTransceiverDirectionSendrecv), ErrRegisterHeaderExtensionInvalidDirection)
        assert.Error(t, m.RegisterHeaderExtension(RTPHeaderExtensionCapability{"pion-header-test"}, RTPCodecTypeAudio, RTPTransceiverDirectionInactive), ErrRegisterHeaderExtensionInvalidDirection)
        assert.Error(t, m.RegisterHeaderExtension(RTPHeaderExtensionCapability{"pion-header-test"}, RTPCodecTypeAudio, RTPTransceiverDirection(0)), ErrRegisterHeaderExtensionInvalidDirection)
    })
}

// If a user attempts to register a codec twice we should just discard duplicate calls
func TestMediaEngineDoubleRegister()->Result<()> {
    m := MediaEngine{}

    assert.NoError(t, m.RegisterCodec(
        RTPCodecParameters{
            RTPCodecCapability: RTPCodecCapability{MimeTypeOpus, 48000, 0, "", nil},
            PayloadType:        111,
        }, RTPCodecTypeAudio))

    assert.NoError(t, m.RegisterCodec(
        RTPCodecParameters{
            RTPCodecCapability: RTPCodecCapability{MimeTypeOpus, 48000, 0, "", nil},
            PayloadType:        111,
        }, RTPCodecTypeAudio))

    assert.Equal(t, len(m.audioCodecs), 1)
}

// The cloned MediaEngine instance should be able to update negotiated header extensions.
func TestUpdateHeaderExtenstionToClonedMediaEngine()->Result<()> {
    src := MediaEngine{}

    assert.NoError(t, src.RegisterCodec(
        RTPCodecParameters{
            RTPCodecCapability: RTPCodecCapability{MimeTypeOpus, 48000, 0, "", nil},
            PayloadType:        111,
        }, RTPCodecTypeAudio))

    assert.NoError(t, src.RegisterHeaderExtension(RTPHeaderExtensionCapability{"test-extension"}, RTPCodecTypeAudio))

    validate := func(m *MediaEngine) {
        assert.NoError(t, m.updateHeaderExtension(2, "test-extension", RTPCodecTypeAudio))

        id, audioNegotiated, videoNegotiated := m.getHeaderExtensionID(RTPHeaderExtensionCapability{URI: "test-extension"})
        assert.Equal(t, 2, id)
        assert.True(t, audioNegotiated)
        assert.False(t, videoNegotiated)
    }

    validate(&src)
    validate(src.copy())
}
*/
