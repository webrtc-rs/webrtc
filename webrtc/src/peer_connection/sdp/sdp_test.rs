use super::*;
use crate::api::media_engine::{MIME_TYPE_OPUS, MIME_TYPE_VP8};
use crate::api::setting_engine::SettingEngine;
use crate::api::APIBuilder;
use crate::dtls_transport::dtls_role::DEFAULT_DTLS_ROLE_OFFER;
use crate::dtls_transport::RTCDtlsTransport;
use crate::peer_connection::certificate::RTCCertificate;
use crate::rtp_transceiver::rtp_sender::RTCRtpSender;
use crate::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use crate::track::track_local::TrackLocal;
use rcgen::KeyPair;
use sdp::description::common::Attribute;

#[test]
fn test_extract_fingerprint() -> Result<()> {
    //"Good Session Fingerprint"
    {
        let s = SessionDescription {
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
        let s = SessionDescription {
            media_descriptions: vec![MediaDescription {
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
        let s = SessionDescription::default();

        if let Err(err) = extract_fingerprint(&s) {
            assert_eq!(err, Error::ErrSessionDescriptionNoFingerprint);
        } else {
            panic!();
        }
    }

    //"Invalid Fingerprint"
    {
        let s = SessionDescription {
            attributes: vec![Attribute {
                key: "fingerprint".to_owned(),
                value: Some("foo".to_owned()),
            }],
            ..Default::default()
        };

        if let Err(err) = extract_fingerprint(&s) {
            assert_eq!(err, Error::ErrSessionDescriptionInvalidFingerprint);
        } else {
            panic!();
        }
    }

    //"Conflicting Fingerprint"
    {
        let s = SessionDescription {
            attributes: vec![Attribute {
                key: "fingerprint".to_owned(),
                value: Some("foo".to_owned()),
            }],
            media_descriptions: vec![MediaDescription {
                attributes: vec![Attribute {
                    key: "fingerprint".to_owned(),
                    value: Some("foo bar".to_owned()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        if let Err(err) = extract_fingerprint(&s) {
            assert_eq!(err, Error::ErrSessionDescriptionConflictingFingerprints);
        } else {
            panic!();
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
        let s = SessionDescription {
            media_descriptions: vec![MediaDescription {
                attributes: vec![Attribute {
                    key: "ice-ufrag".to_owned(),
                    value: Some(DEFAULT_UFRAG.to_owned()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        if let Err(err) = extract_ice_details(&s).await {
            assert_eq!(err, Error::ErrSessionDescriptionMissingIcePwd);
        } else {
            panic!();
        }
    }

    //"Missing ice-ufrag"
    {
        let s = SessionDescription {
            media_descriptions: vec![MediaDescription {
                attributes: vec![Attribute {
                    key: "ice-pwd".to_owned(),
                    value: Some(DEFAULT_PWD.to_owned()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        if let Err(err) = extract_ice_details(&s).await {
            assert_eq!(err, Error::ErrSessionDescriptionMissingIceUfrag);
        } else {
            panic!();
        }
    }

    //"ice details at session level"
    {
        let s = SessionDescription {
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
        let s = SessionDescription {
            media_descriptions: vec![MediaDescription {
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
        let s = SessionDescription {
            attributes: vec![Attribute {
                key: "ice-ufrag".to_owned(),
                value: Some("invalidUfrag".to_owned()),
            }],
            media_descriptions: vec![MediaDescription {
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
            assert_eq!(err, Error::ErrSessionDescriptionConflictingIceUfrag);
        } else {
            panic!();
        }
    }

    //"Conflict pwd"
    {
        let s = SessionDescription {
            attributes: vec![Attribute {
                key: "ice-pwd".to_owned(),
                value: Some("invalidPwd".to_owned()),
            }],
            media_descriptions: vec![MediaDescription {
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
            assert_eq!(err, Error::ErrSessionDescriptionConflictingIcePwd);
        } else {
            panic!();
        }
    }

    Ok(())
}

#[test]
fn test_track_details_from_sdp() -> Result<()> {
    //"Tracks unknown, audio and video with RTX"
    {
        let s = SessionDescription {
            media_descriptions: vec![
                MediaDescription {
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
                MediaDescription {
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
                MediaDescription {
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
                MediaDescription {
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
                MediaDescription {
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
                            key: SDP_ATTRIBUTE_RID.to_owned(),
                            value: Some("f send pt=97;max-width=1280;max-height=720".to_owned()),
                        },
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let tracks = track_details_from_sdp(&s, true);
        assert_eq!(tracks.len(), 3);
        if track_details_for_ssrc(&tracks, 1000).is_some() {
            panic!("got the unknown track ssrc:1000 which should have been skipped");
        }
        if let Some(track) = track_details_for_ssrc(&tracks, 2000) {
            assert_eq!(track.kind, RTPCodecType::Audio);
            assert_eq!(track.ssrcs[0], 2000);
            assert_eq!(track.stream_id, "audio_trk_label");
        } else {
            panic!("missing audio track with ssrc:2000");
        }
        if let Some(track) = track_details_for_ssrc(&tracks, 3000) {
            assert_eq!(track.kind, RTPCodecType::Video);
            assert_eq!(track.ssrcs[0], 3000);
            assert_eq!(track.stream_id, "video_trk_label");
        } else {
            panic!("missing video track with ssrc:3000");
        }
        if track_details_for_ssrc(&tracks, 4000).is_some() {
            panic!("got the rtx track ssrc:3000 which should have been skipped");
        }
        if let Some(track) = track_details_for_ssrc(&tracks, 5000) {
            assert_eq!(track.kind, RTPCodecType::Video);
            assert_eq!(track.ssrcs[0], 5000);
            assert_eq!(track.id, "video_trk_id");
            assert_eq!(track.stream_id, "video_stream_id");
        } else {
            panic!("missing video track with ssrc:5000");
        }
    }

    {
        let s = SessionDescription {
            media_descriptions: vec![
                MediaDescription {
                    media_name: MediaName {
                        media: "video".to_owned(),
                        ..Default::default()
                    },
                    attributes: vec![
                        Attribute {
                            key: "mid".to_owned(),
                            value: Some("1".to_owned()),
                        },
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
                MediaDescription {
                    media_name: MediaName {
                        media: "video".to_owned(),
                        ..Default::default()
                    },
                    attributes: vec![
                        Attribute {
                            key: "mid".to_owned(),
                            value: Some("1".to_owned()),
                        },
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
        assert_eq!(
            track_details_from_sdp(&s, true).len(),
            0,
            "inactive and recvonly tracks should be ignored when passing exclude_inactive: true"
        );
        assert_eq!(
            track_details_from_sdp(&s, false).len(),
            1,
            "Inactive tracks should not be ignored when passing exclude_inactive: false"
        );
    }

    Ok(())
}

#[test]
fn test_have_application_media_section() -> Result<()> {
    //"Audio only"
    {
        let s = SessionDescription {
            media_descriptions: vec![MediaDescription {
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
        let s = SessionDescription {
            media_descriptions: vec![MediaDescription {
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

async fn fingerprint_test(
    certificate: &RTCCertificate,
    engine: &Arc<MediaEngine>,
    media: &[MediaSection],
    sdpmedia_description_fingerprints: bool,
    expected_fingerprint_count: usize,
) -> Result<()> {
    let s = SessionDescription::default();

    let dtls_fingerprints = certificate.get_fingerprints();

    let params = PopulateSdpParams {
        media_description_fingerprint: sdpmedia_description_fingerprints,
        is_icelite: false,
        connection_role: ConnectionRole::Active,
        ice_gathering_state: RTCIceGatheringState::New,
    };

    let s = populate_sdp(
        s,
        &dtls_fingerprints,
        engine,
        &[],
        &RTCIceParameters::default(),
        media,
        params,
    )
    .await?;

    let sdparray = s.marshal();

    assert_eq!(
        sdparray.matches("sha-256").count(),
        expected_fingerprint_count
    );

    Ok(())
}

#[tokio::test]
async fn test_media_description_fingerprints() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();
    let interceptor = api.interceptor_registry.build("")?;

    let kp = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let certificate = RTCCertificate::from_key_pair(kp)?;

    let transport = Arc::new(RTCDtlsTransport::default());

    let video_receiver = Arc::new(api.new_rtp_receiver(
        RTPCodecType::Video,
        Arc::clone(&transport),
        Arc::clone(&interceptor),
    ));
    let audio_receiver = Arc::new(api.new_rtp_receiver(
        RTPCodecType::Audio,
        Arc::clone(&transport),
        Arc::clone(&interceptor),
    ));

    let video_sender = Arc::new(
        api.new_rtp_sender(None, Arc::clone(&transport), Arc::clone(&interceptor))
            .await,
    );

    let audio_sender = Arc::new(
        api.new_rtp_sender(None, Arc::clone(&transport), Arc::clone(&interceptor))
            .await,
    );

    let media = vec![
        MediaSection {
            id: "video".to_owned(),
            transceivers: vec![
                RTCRtpTransceiver::new(
                    video_receiver,
                    video_sender,
                    RTCRtpTransceiverDirection::Inactive,
                    RTPCodecType::Video,
                    api.media_engine
                        .get_codecs_by_kind(RTPCodecType::Video)
                        .await,
                    Arc::clone(&api.media_engine),
                    None,
                )
                .await,
            ],
            ..Default::default()
        },
        MediaSection {
            id: "audio".to_owned(),
            transceivers: vec![
                RTCRtpTransceiver::new(
                    audio_receiver,
                    audio_sender,
                    RTCRtpTransceiverDirection::Inactive,
                    RTPCodecType::Audio,
                    api.media_engine
                        .get_codecs_by_kind(RTPCodecType::Audio)
                        .await,
                    Arc::clone(&api.media_engine),
                    None,
                )
                .await,
            ],
            ..Default::default()
        },
        MediaSection {
            id: "application".to_owned(),
            data: true,
            ..Default::default()
        },
    ];

    #[allow(clippy::needless_range_loop)]
    for i in 0..2 {
        let track: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "video/vp8".to_owned(),
                ..Default::default()
            },
            "video".to_owned(),
            "webrtc-rs".to_owned(),
        ));
        media[i].transceivers[0]
            .set_sender(Arc::new(
                RTCRtpSender::new(
                    api.setting_engine.get_receive_mtu(),
                    Some(track),
                    Arc::new(RTCDtlsTransport::default()),
                    Arc::clone(&api.media_engine),
                    Arc::clone(&interceptor),
                    false,
                )
                .await,
            ))
            .await;
        media[i].transceivers[0].set_direction_internal(RTCRtpTransceiverDirection::Sendonly);
    }

    //"Per-Media Description Fingerprints",
    fingerprint_test(&certificate, &api.media_engine, &media, true, 3).await?;

    //"Per-Session Description Fingerprints",
    fingerprint_test(&certificate, &api.media_engine, &media, false, 1).await?;

    Ok(())
}

#[tokio::test]
async fn test_populate_sdp() -> Result<()> {
    //"Rid"
    {
        let se = SettingEngine::default();
        let mut me = MediaEngine::default();
        me.register_default_codecs()?;

        let api = APIBuilder::new().with_media_engine(me).build();
        let interceptor = api.interceptor_registry.build("")?;
        let transport = Arc::new(RTCDtlsTransport::default());

        let receiver = Arc::new(api.new_rtp_receiver(
            RTPCodecType::Video,
            Arc::clone(&transport),
            Arc::clone(&interceptor),
        ));

        let sender = Arc::new(
            api.new_rtp_sender(None, Arc::clone(&transport), Arc::clone(&interceptor))
                .await,
        );

        let tr = RTCRtpTransceiver::new(
            receiver,
            sender,
            RTCRtpTransceiverDirection::Recvonly,
            RTPCodecType::Video,
            api.media_engine.video_codecs.clone(),
            Arc::clone(&api.media_engine),
            None,
        )
        .await;

        let mut rid_map = HashMap::new();
        rid_map.insert("ridkey".to_owned(), "some".to_owned());
        let media_sections = vec![MediaSection {
            id: "video".to_owned(),
            transceivers: vec![tr],
            data: false,
            rid_map,
            ..Default::default()
        }];

        let d = SessionDescription::default();

        let params = PopulateSdpParams {
            media_description_fingerprint: se.sdp_media_level_fingerprints,
            is_icelite: se.candidates.ice_lite,
            connection_role: DEFAULT_DTLS_ROLE_OFFER.to_connection_role(),
            ice_gathering_state: RTCIceGatheringState::Complete,
        };
        let offer_sdp = populate_sdp(
            d,
            &[],
            &api.media_engine,
            &[],
            &RTCIceParameters::default(),
            &media_sections,
            params,
        )
        .await?;

        // Test contains rid map keys
        let mut found = false;
        for desc in &offer_sdp.media_descriptions {
            if desc.media_name.media != "video" {
                continue;
            }
            for a in &desc.attributes {
                if a.key == SDP_ATTRIBUTE_RID {
                    if let Some(value) = &a.value {
                        if value.contains("ridkey") {
                            found = true;
                            break;
                        }
                    }
                }
            }
        }
        assert!(found, "Rid key should be present");
    }

    //"SetCodecPreferences"
    {
        let se = SettingEngine::default();
        let mut me = MediaEngine::default();
        me.register_default_codecs()?;
        me.push_codecs(me.video_codecs.clone(), RTPCodecType::Video)
            .await;
        me.push_codecs(me.audio_codecs.clone(), RTPCodecType::Audio)
            .await;

        let api = APIBuilder::new().with_media_engine(me).build();
        let interceptor = api.interceptor_registry.build("")?;
        let transport = Arc::new(RTCDtlsTransport::default());
        let receiver = Arc::new(api.new_rtp_receiver(
            RTPCodecType::Video,
            Arc::clone(&transport),
            Arc::clone(&interceptor),
        ));

        let sender = Arc::new(
            api.new_rtp_sender(None, Arc::clone(&transport), Arc::clone(&interceptor))
                .await,
        );

        let tr = RTCRtpTransceiver::new(
            receiver,
            sender,
            RTCRtpTransceiverDirection::Recvonly,
            RTPCodecType::Video,
            api.media_engine.video_codecs.clone(),
            Arc::clone(&api.media_engine),
            None,
        )
        .await;
        tr.set_codec_preferences(vec![RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 96,
            ..Default::default()
        }])
        .await?;

        let media_sections = vec![MediaSection {
            id: "video".to_owned(),
            transceivers: vec![tr],
            data: false,
            rid_map: HashMap::new(),
            ..Default::default()
        }];

        let d = SessionDescription::default();

        let params = PopulateSdpParams {
            media_description_fingerprint: se.sdp_media_level_fingerprints,
            is_icelite: se.candidates.ice_lite,
            connection_role: DEFAULT_DTLS_ROLE_OFFER.to_connection_role(),
            ice_gathering_state: RTCIceGatheringState::Complete,
        };
        let offer_sdp = populate_sdp(
            d,
            &[],
            &api.media_engine,
            &[],
            &RTCIceParameters::default(),
            &media_sections,
            params,
        )
        .await?;

        // Test codecs
        let mut found_vp8 = false;
        for desc in &offer_sdp.media_descriptions {
            if desc.media_name.media != "video" {
                continue;
            }
            for a in &desc.attributes {
                if a.key.contains("rtpmap") {
                    if let Some(value) = &a.value {
                        if value == "98 VP9/90000" {
                            panic!("vp9 should not be present in sdp");
                        } else if value == "96 VP8/90000" {
                            found_vp8 = true;
                        }
                    }
                }
            }
        }
        assert!(found_vp8, "vp8 should be present in sdp");
    }

    Ok(())
}

#[tokio::test]
async fn test_populate_sdp_reject() -> Result<()> {
    let se = SettingEngine::default();
    let mut me = MediaEngine::default();
    me.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90_000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 2,
            stats_id: "id".to_owned(),
        },
        RTPCodecType::Video,
    )?;

    let api = APIBuilder::new().with_media_engine(me).build();
    let interceptor = api.interceptor_registry.build("")?;
    let transport = Arc::new(RTCDtlsTransport::default());
    let video_receiver = Arc::new(api.new_rtp_receiver(
        RTPCodecType::Video,
        Arc::clone(&transport),
        Arc::clone(&interceptor),
    ));

    let video_sender = Arc::new(
        api.new_rtp_sender(None, Arc::clone(&transport), Arc::clone(&interceptor))
            .await,
    );

    let trv = RTCRtpTransceiver::new(
        video_receiver,
        video_sender,
        RTCRtpTransceiverDirection::Recvonly,
        RTPCodecType::Video,
        api.media_engine.video_codecs.clone(),
        Arc::clone(&api.media_engine),
        None,
    )
    .await;

    let audio_receiver = Arc::new(api.new_rtp_receiver(
        RTPCodecType::Audio,
        Arc::clone(&transport),
        Arc::clone(&interceptor),
    ));

    let audio_sender = Arc::new(
        api.new_rtp_sender(None, Arc::clone(&transport), Arc::clone(&interceptor))
            .await,
    );

    let tra = RTCRtpTransceiver::new(
        audio_receiver,
        audio_sender,
        RTCRtpTransceiverDirection::Recvonly,
        RTPCodecType::Audio,
        api.media_engine.audio_codecs.clone(),
        Arc::clone(&api.media_engine),
        None,
    )
    .await;

    let media_sections = vec![
        MediaSection {
            id: "video".to_owned(),
            transceivers: vec![trv],
            data: false,
            rid_map: HashMap::new(),
            ..Default::default()
        },
        MediaSection {
            id: "audio".to_owned(),
            transceivers: vec![tra],
            data: false,
            rid_map: HashMap::new(),
            ..Default::default()
        },
    ];

    let d = SessionDescription::default();

    let params = PopulateSdpParams {
        media_description_fingerprint: se.sdp_media_level_fingerprints,
        is_icelite: se.candidates.ice_lite,
        connection_role: DEFAULT_DTLS_ROLE_OFFER.to_connection_role(),
        ice_gathering_state: RTCIceGatheringState::Complete,
    };
    let offer_sdp = populate_sdp(
        d,
        &[],
        &api.media_engine,
        &[],
        &RTCIceParameters::default(),
        &media_sections,
        params,
    )
    .await?;

    let mut found_rejected_track = false;

    for desc in offer_sdp.media_descriptions {
        if desc.media_name.media != "audio" {
            continue;
        }
        found_rejected_track = true;

        assert!(
            desc.connection_information.is_some(),
            "connection_information should not be None, even for rejected tracks"
        );
        assert_eq!(
            desc.media_name.formats,
            vec!["0"],
            "Format for rejected track should be 0"
        );
        assert_eq!(
            desc.media_name.port.value, 0,
            "Port for rejected track should be 0"
        );
    }

    assert!(
        found_rejected_track,
        "There should've been a rejected track"
    );

    Ok(())
}

#[test]
fn test_get_rids() {
    let m = vec![MediaDescription {
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
                key: SDP_ATTRIBUTE_RID.to_owned(),
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
        let codecs = codecs_from_media_description(&MediaDescription {
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
            vec![RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
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
        let codecs = codecs_from_media_description(&MediaDescription {
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
            vec![RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
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
    let extensions = rtp_extensions_from_media_description(&MediaDescription {
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
