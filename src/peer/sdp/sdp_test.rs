use super::*;
use crate::api::media_engine::MIME_TYPE_OPUS;
use sdp::common_description::Attribute;

#[test]
fn test_extract_fingerprint() -> Result<()> {
    //"Good Session Fingerprint"
    {
        let s = sdp::session_description::SessionDescription {
            attributes: vec![Attribute {
                key: "fingerprint".to_owned(),
                value: Some("foo bar".to_owned()),
            }],
            ..Default::default()
        };

        let (fingerprint, hash) = extract_fingerprint(&s)?;
        assert_eq!(fingerprint, "bar");
        assert_eq!(hash, "foo");
    }

    //"Good Media Fingerprint"
    {
        let s = sdp::session_description::SessionDescription {
            media_descriptions: vec![sdp::media_description::MediaDescription {
                attributes: vec![Attribute {
                    key: "fingerprint".to_owned(),
                    value: Some("foo bar".to_owned()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        let (fingerprint, hash) = extract_fingerprint(&s)?;
        assert_eq!(fingerprint, "bar");
        assert_eq!(hash, "foo");
    }

    //"No Fingerprint"
    {
        let s = sdp::session_description::SessionDescription::default();

        if let Err(err) = extract_fingerprint(&s) {
            assert!(Error::ErrSessionDescriptionNoFingerprint.equal(&err));
        } else {
            assert!(false);
        }
    }

    //"Invalid Fingerprint"
    {
        let s = sdp::session_description::SessionDescription {
            attributes: vec![Attribute {
                key: "fingerprint".to_owned(),
                value: Some("foo".to_owned()),
            }],
            ..Default::default()
        };

        if let Err(err) = extract_fingerprint(&s) {
            assert!(Error::ErrSessionDescriptionInvalidFingerprint.equal(&err));
        } else {
            assert!(false);
        }
    }

    //"Conflicting Fingerprint"
    {
        let s = sdp::session_description::SessionDescription {
            attributes: vec![Attribute {
                key: "fingerprint".to_owned(),
                value: Some("foo".to_owned()),
            }],
            media_descriptions: vec![sdp::media_description::MediaDescription {
                attributes: vec![Attribute {
                    key: "fingerprint".to_owned(),
                    value: Some("foo bar".to_owned()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        if let Err(err) = extract_fingerprint(&s) {
            assert!(Error::ErrSessionDescriptionConflictingFingerprints.equal(&err));
        } else {
            assert!(false);
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_extract_ice_details() -> Result<()> {
    const DEFAULT_UFRAG: &str = "DEFAULT_PWD";
    const DEFAULT_PWD: &str = "DEFAULT_UFRAG";

    //"Missing ice-pwd"
    {
        let s = sdp::session_description::SessionDescription {
            media_descriptions: vec![sdp::media_description::MediaDescription {
                attributes: vec![Attribute {
                    key: "ice-ufrag".to_owned(),
                    value: Some(DEFAULT_UFRAG.to_owned()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        if let Err(err) = extract_ice_details(&s).await {
            assert!(Error::ErrSessionDescriptionMissingIcePwd.equal(&err));
        } else {
            assert!(false);
        }
    }

    //"Missing ice-ufrag"
    {
        let s = sdp::session_description::SessionDescription {
            media_descriptions: vec![sdp::media_description::MediaDescription {
                attributes: vec![Attribute {
                    key: "ice-pwd".to_owned(),
                    value: Some(DEFAULT_PWD.to_owned()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        if let Err(err) = extract_ice_details(&s).await {
            assert!(Error::ErrSessionDescriptionMissingIceUfrag.equal(&err));
        } else {
            assert!(false);
        }
    }

    //"ice details at session level"
    {
        let s = sdp::session_description::SessionDescription {
            attributes: vec![
                Attribute {
                    key: "ice-ufrag".to_owned(),
                    value: Some(DEFAULT_UFRAG.to_owned()),
                },
                Attribute {
                    key: "ice-pwd".to_owned(),
                    value: Some(DEFAULT_PWD.to_owned()),
                },
            ],
            media_descriptions: vec![],
            ..Default::default()
        };

        let (ufrag, pwd, _) = extract_ice_details(&s).await?;
        assert_eq!(ufrag, DEFAULT_UFRAG);
        assert_eq!(pwd, DEFAULT_PWD);
    }

    //"ice details at media level"
    {
        let s = sdp::session_description::SessionDescription {
            media_descriptions: vec![sdp::media_description::MediaDescription {
                attributes: vec![
                    Attribute {
                        key: "ice-ufrag".to_owned(),
                        value: Some(DEFAULT_UFRAG.to_owned()),
                    },
                    Attribute {
                        key: "ice-pwd".to_owned(),
                        value: Some(DEFAULT_PWD.to_owned()),
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        let (ufrag, pwd, _) = extract_ice_details(&s).await?;
        assert_eq!(ufrag, DEFAULT_UFRAG);
        assert_eq!(pwd, DEFAULT_PWD);
    }

    //"Conflict ufrag"
    {
        let s = sdp::session_description::SessionDescription {
            attributes: vec![Attribute {
                key: "ice-ufrag".to_owned(),
                value: Some("invalidUfrag".to_owned()),
            }],
            media_descriptions: vec![sdp::media_description::MediaDescription {
                attributes: vec![
                    Attribute {
                        key: "ice-ufrag".to_owned(),
                        value: Some(DEFAULT_UFRAG.to_owned()),
                    },
                    Attribute {
                        key: "ice-pwd".to_owned(),
                        value: Some(DEFAULT_PWD.to_owned()),
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        if let Err(err) = extract_ice_details(&s).await {
            assert!(Error::ErrSessionDescriptionConflictingIceUfrag.equal(&err));
        } else {
            assert!(false);
        }
    }

    //"Conflict pwd"
    {
        let s = sdp::session_description::SessionDescription {
            attributes: vec![Attribute {
                key: "ice-pwd".to_owned(),
                value: Some("invalidPwd".to_owned()),
            }],
            media_descriptions: vec![sdp::media_description::MediaDescription {
                attributes: vec![
                    Attribute {
                        key: "ice-ufrag".to_owned(),
                        value: Some(DEFAULT_UFRAG.to_owned()),
                    },
                    Attribute {
                        key: "ice-pwd".to_owned(),
                        value: Some(DEFAULT_PWD.to_owned()),
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        if let Err(err) = extract_ice_details(&s).await {
            assert!(Error::ErrSessionDescriptionConflictingIcePwd.equal(&err));
        } else {
            assert!(false);
        }
    }

    Ok(())
}

#[test]
fn test_track_details_from_sdp() -> Result<()> {
    //"Tracks unknown, audio and video with RTX"
    {
        let s = sdp::session_description::SessionDescription {
            media_descriptions: vec![
                sdp::media_description::MediaDescription {
                    media_name: MediaName {
                        media: "foobar".to_owned(),
                        ..Default::default()
                    },
                    attributes: vec![
                        Attribute {
                            key: "mid".to_owned(),
                            value: Some("0".to_owned()),
                        },
                        Attribute {
                            key: "sendrecv".to_owned(),
                            value: None,
                        },
                        Attribute {
                            key: "ssrc".to_owned(),
                            value: Some("1000 msid:unknown_trk_label unknown_trk_guid".to_owned()),
                        },
                    ],
                    ..Default::default()
                },
                sdp::media_description::MediaDescription {
                    media_name: MediaName {
                        media: "audio".to_owned(),
                        ..Default::default()
                    },
                    attributes: vec![
                        Attribute {
                            key: "mid".to_owned(),
                            value: Some("1".to_owned()),
                        },
                        Attribute {
                            key: "sendrecv".to_owned(),
                            value: None,
                        },
                        Attribute {
                            key: "ssrc".to_owned(),
                            value: Some("2000 msid:audio_trk_label audio_trk_guid".to_owned()),
                        },
                    ],
                    ..Default::default()
                },
                sdp::media_description::MediaDescription {
                    media_name: MediaName {
                        media: "video".to_owned(),
                        ..Default::default()
                    },
                    attributes: vec![
                        Attribute {
                            key: "mid".to_owned(),
                            value: Some("2".to_owned()),
                        },
                        Attribute {
                            key: "sendrecv".to_owned(),
                            value: None,
                        },
                        Attribute {
                            key: "ssrc-group".to_owned(),
                            value: Some("FID 3000 4000".to_owned()),
                        },
                        Attribute {
                            key: "ssrc".to_owned(),
                            value: Some("3000 msid:video_trk_label video_trk_guid".to_owned()),
                        },
                        Attribute {
                            key: "ssrc".to_owned(),
                            value: Some("4000 msid:rtx_trk_label rtx_trck_guid".to_owned()),
                        },
                    ],
                    ..Default::default()
                },
                sdp::media_description::MediaDescription {
                    media_name: MediaName {
                        media: "video".to_owned(),
                        ..Default::default()
                    },
                    attributes: vec![
                        Attribute {
                            key: "mid".to_owned(),
                            value: Some("3".to_owned()),
                        },
                        Attribute {
                            key: "sendonly".to_owned(),
                            value: None,
                        },
                        Attribute {
                            key: "msid".to_owned(),
                            value: Some("video_stream_id video_trk_id".to_owned()),
                        },
                        Attribute {
                            key: "ssrc".to_owned(),
                            value: Some("5000".to_owned()),
                        },
                    ],
                    ..Default::default()
                },
                sdp::media_description::MediaDescription {
                    media_name: MediaName {
                        media: "video".to_owned(),
                        ..Default::default()
                    },
                    attributes: vec![
                        Attribute {
                            key: "sendonly".to_owned(),
                            value: None,
                        },
                        Attribute {
                            key: "rid".to_owned(),
                            value: Some("f send pt=97;max-width=1280;max-height=720".to_owned()),
                        },
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let tracks = track_details_from_sdp(&s);
        assert_eq!(3, tracks.len());
        if track_details_for_ssrc(&tracks, 1000).is_some() {
            assert!(
                false,
                "got the unknown track ssrc:1000 which should have been skipped"
            );
        }
        if let Some(track) = track_details_for_ssrc(&tracks, 2000) {
            assert_eq!(RTPCodecType::Audio, track.kind);
            assert_eq!(2000, track.ssrc);
            assert_eq!("audio_trk_label", track.stream_id);
        } else {
            assert!(false, "missing audio track with ssrc:2000");
        }
        if let Some(track) = track_details_for_ssrc(&tracks, 3000) {
            assert_eq!(RTPCodecType::Video, track.kind);
            assert_eq!(3000, track.ssrc);
            assert_eq!("video_trk_label", track.stream_id);
        } else {
            assert!(false, "missing video track with ssrc:3000");
        }
        if track_details_for_ssrc(&tracks, 4000).is_some() {
            assert!(
                false,
                "got the rtx track ssrc:3000 which should have been skipped"
            );
        }
        if let Some(track) = track_details_for_ssrc(&tracks, 5000) {
            assert_eq!(RTPCodecType::Video, track.kind);
            assert_eq!(5000, track.ssrc);
            assert_eq!("video_trk_id", track.id);
            assert_eq!("video_stream_id", track.stream_id);
        } else {
            assert!(false, "missing video track with ssrc:5000");
        }
    }

    //"inactive and recvonly tracks ignored"
    {
        let s = sdp::session_description::SessionDescription {
            media_descriptions: vec![
                sdp::media_description::MediaDescription {
                    media_name: MediaName {
                        media: "video".to_owned(),
                        ..Default::default()
                    },
                    attributes: vec![
                        Attribute {
                            key: "inactive".to_owned(),
                            value: None,
                        },
                        Attribute {
                            key: "ssrc".to_owned(),
                            value: Some("6000".to_owned()),
                        },
                    ],
                    ..Default::default()
                },
                sdp::media_description::MediaDescription {
                    media_name: MediaName {
                        media: "video".to_owned(),
                        ..Default::default()
                    },
                    attributes: vec![
                        Attribute {
                            key: "recvonly".to_owned(),
                            value: None,
                        },
                        Attribute {
                            key: "ssrc".to_owned(),
                            value: Some("7000".to_owned()),
                        },
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(0, track_details_from_sdp(&s).len());
    }

    Ok(())
}

#[test]
fn test_have_application_media_section() -> Result<()> {
    //"Audio only"
    {
        let s = sdp::session_description::SessionDescription {
            media_descriptions: vec![sdp::media_description::MediaDescription {
                media_name: MediaName {
                    media: "audio".to_owned(),
                    ..Default::default()
                },
                attributes: vec![
                    Attribute {
                        key: "sendrecv".to_owned(),
                        value: None,
                    },
                    Attribute {
                        key: "ssrc".to_owned(),
                        value: Some("2000".to_owned()),
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        assert!(!have_application_media_section(&s));
    }

    //"Application"
    {
        let s = sdp::session_description::SessionDescription {
            media_descriptions: vec![sdp::media_description::MediaDescription {
                media_name: MediaName {
                    media: MEDIA_SECTION_APPLICATION.to_owned(),
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };

        assert!(have_application_media_section(&s));
    }

    Ok(())
}

/*TODO:
#[test]
fn test_media_description_fingerprints() -> Result<()> {
    let mut engine = MediaEngine::default();
    engine.register_default_codecs()?;
    let engine = Arc::new(engine);

    let sk := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
    assert.NoError(t, err)

    certificate, err := GenerateCertificate(sk)
    assert.NoError(t, err)

    media := []mediaSection{
        {
            id: "video",
            transceivers: []*RTPTransceiver{{
                kind:   RTPCodecTypeVideo,
                api:    api,
                codecs: engine.getCodecsByKind(RTPCodecTypeVideo),
            }},
        },
        {
            id: "audio",
            transceivers: []*RTPTransceiver{{
                kind:   RTPCodecTypeAudio,
                api:    api,
                codecs: engine.getCodecsByKind(RTPCodecTypeAudio),
            }},
        },
        {
            id:   "application",
            data: true,
        },
    }

    for i := 0; i < 2; i++ {
        media[i].transceivers[0].setSender(&RTPSender{})
        media[i].transceivers[0].setDirection(RTPTransceiverDirectionSendonly)
    }

    fingerprintTest := func(SDPMediaDescriptionFingerprints bool, expectedFingerprintCount int) func(t *testing.T) {
        return func(t *testing.T) {
            s := &sdp.SessionDescription{}

            dtlsFingerprints, err := certificate.GetFingerprints()
            assert.NoError(t, err)

            s, err = populateSDP(s, false,
                dtlsFingerprints,
                SDPMediaDescriptionFingerprints,
                false, engine, sdp.ConnectionRoleActive, []ICECandidate{}, ICEParameters{}, media, ICEGatheringStateNew)
            assert.NoError(t, err)

            sdparray, err := s.Marshal()
            assert.NoError(t, err)

            assert.Equal(t, strings.Count(string(sdparray), "sha-256"), expectedFingerprintCount)
        }
    }

    t.Run("Per-Media Description Fingerprints", fingerprintTest(true, 3))
    t.Run("Per-Session Description Fingerprints", fingerprintTest(false, 1))
}


#[tokio::test]
async fn test_populate_sdp() ->Result<()>{
    //"Rid"
    {
        se := SettingEngine{}

        me := &MediaEngine{}
        assert.NoError(t, me.RegisterDefaultCodecs())
        api := NewAPI(WithMediaEngine(me))

        tr := &RTPTransceiver{kind: RTPCodecTypeVideo, api: api, codecs: me.videoCodecs}
        tr.setDirection(RTPTransceiverDirectionRecvonly)
        ridMap := map[string]string{
            "ridkey": "some",
        }
        mediaSections := []mediaSection{{id: "video", transceivers: []*RTPTransceiver{tr}, ridMap: ridMap}}

        d := &sdp.SessionDescription{}

        offerSdp, err := populateSDP(d, false, []DTLSFingerprint{}, se.sdpMediaLevelFingerprints, se.candidates.ICELite, me, connectionRoleFromDtlsRole(defaultDtlsRoleOffer), []ICECandidate{}, ICEParameters{}, mediaSections, ICEGatheringStateComplete)
        assert.Nil(t, err)

        // Test contains rid map keys
        var found bool
        for _, desc := range offerSdp.MediaDescriptions {
            if desc.MediaName.Media != "video" {
                continue
            }
            for _, a := range desc.Attributes {
                if a.Key == "rid" {
                    if strings.Contains(a.Value, "ridkey") {
                        found = true
                        break
                    }
                }
            }
        }
        assert.Equal(t, true, found, "Rid key should be present")
    }

    //"SetCodecPreferences"
    {
        se := SettingEngine{}

        me := &MediaEngine{}
        assert.NoError(t, me.RegisterDefaultCodecs())
        api := NewAPI(WithMediaEngine(me))
        me.pushCodecs(me.videoCodecs, RTPCodecTypeVideo)
        me.pushCodecs(me.audioCodecs, RTPCodecTypeAudio)

        tr := &RTPTransceiver{kind: RTPCodecTypeVideo, api: api, codecs: me.videoCodecs}
        tr.setDirection(RTPTransceiverDirectionRecvonly)
        codecErr := tr.SetCodecPreferences([]RTPCodecParameters{
            {
                RTPCodecCapability: RTPCodecCapability{MimeTypeVP8, 90000, 0, "", nil},
                PayloadType:        96,
            },
        })
        assert.NoError(t, codecErr)

        mediaSections := []mediaSection{{id: "video", transceivers: []*RTPTransceiver{tr}}}

        d := &sdp.SessionDescription{}

        offerSdp, err := populateSDP(d, false, []DTLSFingerprint{}, se.sdpMediaLevelFingerprints, se.candidates.ICELite, me, connectionRoleFromDtlsRole(defaultDtlsRoleOffer), []ICECandidate{}, ICEParameters{}, mediaSections, ICEGatheringStateComplete)
        assert.Nil(t, err)

        // Test codecs
        foundVP8 := false
        for _, desc := range offerSdp.MediaDescriptions {
            if desc.MediaName.Media != "video" {
                continue
            }
            for _, a := range desc.Attributes {
                if strings.Contains(a.Key, "rtpmap") {
                    if a.Value == "98 VP9/90000" {
                        t.Fatal("vp9 should not be present in sdp")
                    } else if a.Value == "96 VP8/90000" {
                        foundVP8 = true
                    }
                }
            }
        }
        assert.Equal(t, true, foundVP8, "vp8 should be present in sdp")
    }

    Ok(())
}
 */

#[test]
fn test_get_rids() {
    let m = vec![sdp::media_description::MediaDescription {
        media_name: MediaName {
            media: "video".to_owned(),
            ..Default::default()
        },
        attributes: vec![
            Attribute {
                key: "sendonly".to_owned(),
                value: None,
            },
            Attribute {
                key: "rid".to_owned(),
                value: Some("f send pt=97;max-width=1280;max-height=720".to_owned()),
            },
        ],
        ..Default::default()
    }];

    let rids = get_rids(&m[0]);

    assert!(!rids.is_empty(), "Rid mapping should be present");

    assert!(rids.get("f").is_some(), "rid values should contain 'f'");
}

#[test]
fn test_codecs_from_media_description() -> Result<()> {
    //"Codec Only"
    {
        let codecs = codecs_from_media_description(&sdp::media_description::MediaDescription {
            media_name: MediaName {
                media: "audio".to_owned(),
                formats: vec!["111".to_owned()],
                ..Default::default()
            },
            attributes: vec![Attribute {
                key: "rtpmap".to_owned(),
                value: Some("111 opus/48000/2".to_owned()),
            }],
            ..Default::default()
        })?;

        assert_eq!(
            codecs,
            vec![RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 111,
                ..Default::default()
            }],
        );
    }

    //"Codec with fmtp/rtcp-fb"
    {
        let codecs = codecs_from_media_description(&sdp::media_description::MediaDescription {
            media_name: MediaName {
                media: "audio".to_owned(),
                formats: vec!["111".to_owned()],
                ..Default::default()
            },
            attributes: vec![
                Attribute {
                    key: "rtpmap".to_owned(),
                    value: Some("111 opus/48000/2".to_owned()),
                },
                Attribute {
                    key: "fmtp".to_owned(),
                    value: Some("111 minptime=10;useinbandfec=1".to_owned()),
                },
                Attribute {
                    key: "rtcp-fb".to_owned(),
                    value: Some("111 goog-remb".to_owned()),
                },
                Attribute {
                    key: "rtcp-fb".to_owned(),
                    value: Some("111 ccm fir".to_owned()),
                },
            ],
            ..Default::default()
        })?;

        assert_eq!(
            codecs,
            vec![RTPCodecParameters {
                capability: RTPCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                    rtcp_feedback: vec![
                        RTCPFeedback {
                            typ: "goog-remb".to_owned(),
                            parameter: "".to_owned()
                        },
                        RTCPFeedback {
                            typ: "ccm".to_owned(),
                            parameter: "fir".to_owned()
                        }
                    ]
                },
                payload_type: 111,
                ..Default::default()
            }],
        );
    }

    Ok(())
}

#[test]
fn test_rtp_extensions_from_media_description() -> Result<()> {
    let extensions =
        rtp_extensions_from_media_description(&sdp::media_description::MediaDescription {
            media_name: MediaName {
                media: "audio".to_owned(),
                formats: vec!["111".to_owned()],
                ..Default::default()
            },
            attributes: vec![
                Attribute {
                    key: "extmap".to_owned(),
                    value: Some("1 ".to_owned() + sdp::extmap::ABS_SEND_TIME_URI),
                },
                Attribute {
                    key: "extmap".to_owned(),
                    value: Some("3 ".to_owned() + sdp::extmap::SDES_MID_URI),
                },
            ],
            ..Default::default()
        })?;

    assert_eq!(extensions[sdp::extmap::ABS_SEND_TIME_URI], 1);
    assert_eq!(extensions[sdp::extmap::SDES_MID_URI], 3);

    Ok(())
}
