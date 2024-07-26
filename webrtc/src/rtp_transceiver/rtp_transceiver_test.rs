use portable_atomic::AtomicUsize;

use super::*;
use crate::api::media_engine::{MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9};
use crate::api::APIBuilder;
use crate::dtls_transport::RTCDtlsTransport;
use crate::peer_connection::configuration::RTCConfiguration;
use crate::peer_connection::peer_connection_test::{close_pair_now, create_vnet_pair};

#[tokio::test]
async fn test_rtp_transceiver_set_codec_preferences() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    m.push_codecs(m.video_codecs.clone(), RTPCodecType::Video)
        .await;
    m.push_codecs(m.audio_codecs.clone(), RTPCodecType::Audio)
        .await;

    let media_video_codecs = m.video_codecs.clone();

    let api = APIBuilder::new().with_media_engine(m).build();
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
        RTCRtpTransceiverDirection::Unspecified,
        RTPCodecType::Video,
        media_video_codecs.clone(),
        Arc::clone(&api.media_engine),
        None,
    )
    .await;

    assert_eq!(&tr.get_codecs().await, &media_video_codecs);

    let fail_test_cases = vec![
        vec![RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_string(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_string(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        }],
        vec![
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_string(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_string(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 111,
                ..Default::default()
            },
        ],
    ];

    for test_case in fail_test_cases {
        if let Err(err) = tr.set_codec_preferences(test_case).await {
            assert_eq!(err, Error::ErrRTPTransceiverCodecUnsupported);
        } else {
            panic!();
        }
    }

    let success_test_cases = vec![
        vec![RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_string(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_string(),
                rtcp_feedback: vec![],
            },
            payload_type: 96,
            ..Default::default()
        }],
        vec![
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_string(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
                ..Default::default()
            },
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_VP9.to_string(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "profile-id=0".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 98,
                ..Default::default()
            },
        ],
    ];

    for test_case in success_test_cases {
        tr.set_codec_preferences(test_case).await?;
    }

    tr.set_codec_preferences(vec![]).await?;
    assert_ne!(0, tr.get_codecs().await.len());

    Ok(())
}

// Assert that SetCodecPreferences properly filters codecs and PayloadTypes are respected
#[tokio::test]
async fn test_rtp_transceiver_set_codec_preferences_payload_type() -> Result<()> {
    let test_codec = RTCRtpCodecParameters {
        capability: RTCRtpCodecCapability {
            mime_type: "video/test_codec".to_string(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_string(),
            rtcp_feedback: vec![],
        },
        payload_type: 50,
        ..Default::default()
    };

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();
    let offer_pc = api.new_peer_connection(RTCConfiguration::default()).await?;

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    m.register_codec(test_codec.clone(), RTPCodecType::Video)?;
    let api = APIBuilder::new().with_media_engine(m).build();
    let answer_pc = api.new_peer_connection(RTCConfiguration::default()).await?;

    let _ = offer_pc
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    let answer_transceiver = answer_pc
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    answer_transceiver
        .set_codec_preferences(vec![
            test_codec,
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_VP8.to_string(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 51,
                ..Default::default()
            },
        ])
        .await?;

    let offer = offer_pc.create_offer(None).await?;

    offer_pc.set_local_description(offer.clone()).await?;
    answer_pc.set_remote_description(offer).await?;

    let answer = answer_pc.create_answer(None).await?;

    // VP8 with proper PayloadType
    assert!(
        answer.sdp.contains("a=rtpmap:51 VP8/90000"),
        "{}",
        answer.sdp
    );

    // test_codec is ignored since offerer doesn't support
    assert!(!answer.sdp.contains("test_codec"));

    close_pair_now(&offer_pc, &answer_pc).await;

    Ok(())
}

#[tokio::test]
async fn test_rtp_transceiver_direction_change() -> Result<()> {
    let (offer_pc, answer_pc, _) = create_vnet_pair().await?;

    let offer_transceiver = offer_pc
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    let _ = answer_pc
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    let offer = offer_pc.create_offer(None).await?;

    offer_pc.set_local_description(offer.clone()).await?;
    answer_pc.set_remote_description(offer).await?;

    let answer = answer_pc.create_answer(None).await?;
    assert!(answer.sdp.contains("a=sendrecv"),);
    answer_pc.set_local_description(answer.clone()).await?;
    offer_pc.set_remote_description(answer).await?;

    offer_transceiver
        .set_direction(RTCRtpTransceiverDirection::Inactive)
        .await;

    let offer = offer_pc.create_offer(None).await?;
    assert!(offer.sdp.contains("a=inactive"),);

    offer_pc.set_local_description(offer.clone()).await?;
    answer_pc.set_remote_description(offer).await?;

    let answer = answer_pc.create_answer(None).await?;
    assert!(answer.sdp.contains("a=inactive"),);
    offer_pc.set_remote_description(answer).await?;

    close_pair_now(&offer_pc, &answer_pc).await;

    Ok(())
}

#[tokio::test]
async fn test_rtp_transceiver_set_direction_causing_negotiation() -> Result<()> {
    let (offer_pc, answer_pc, _) = create_vnet_pair().await?;

    let count = Arc::new(AtomicUsize::new(0));

    {
        let count = count.clone();
        offer_pc.on_negotiation_needed(Box::new(move || {
            let count = count.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
            })
        }));
    }

    let offer_transceiver = offer_pc
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    let _ = answer_pc
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    let offer = offer_pc.create_offer(None).await?;
    offer_pc.set_local_description(offer.clone()).await?;
    answer_pc.set_remote_description(offer).await?;

    let answer = answer_pc.create_answer(None).await?;
    answer_pc.set_local_description(answer.clone()).await?;
    offer_pc.set_remote_description(answer).await?;

    assert_eq!(count.load(Ordering::SeqCst), 0);

    let offer = offer_pc.create_offer(None).await?;
    offer_pc.set_local_description(offer.clone()).await?;
    answer_pc.set_remote_description(offer).await?;

    let answer = answer_pc.create_answer(None).await?;
    answer_pc.set_local_description(answer.clone()).await?;
    offer_pc.set_remote_description(answer).await?;

    assert_eq!(count.load(Ordering::SeqCst), 0);

    offer_transceiver
        .set_direction(RTCRtpTransceiverDirection::Inactive)
        .await;

    // wait for negotiation ops queue to finish.
    offer_pc.internal.ops.done().await;

    assert_eq!(count.load(Ordering::SeqCst), 1);

    close_pair_now(&offer_pc, &answer_pc).await;

    Ok(())
}

#[ignore]
#[tokio::test]
async fn test_rtp_transceiver_stopping() -> Result<()> {
    let (offer_pc, answer_pc, _) = create_vnet_pair().await?;

    let offer_transceiver = offer_pc
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    let _ = answer_pc
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    let offer = offer_pc.create_offer(None).await?;

    offer_pc.set_local_description(offer.clone()).await?;
    answer_pc.set_remote_description(offer).await?;

    let answer = answer_pc.create_answer(None).await?;
    assert!(answer.sdp.contains("a=sendrecv"),);
    answer_pc.set_local_description(answer.clone()).await?;
    offer_pc.set_remote_description(answer).await?;

    assert!(
        offer_transceiver.mid().is_some(),
        "A mid should have been associated with the transceiver when applying the answer"
    );
    // Stop the transceiver
    offer_transceiver.stop().await?;

    let offer = offer_pc.create_offer(None).await?;
    assert!(offer.sdp.contains("a=inactive"),);
    let parsed = offer.parsed.unwrap();
    let m = &parsed.media_descriptions[0];
    assert_eq!(
        m.media_name.port.value, 0,
        "After stopping a transceiver it should be rejected in offers"
    );

    close_pair_now(&offer_pc, &answer_pc).await;

    Ok(())
}
