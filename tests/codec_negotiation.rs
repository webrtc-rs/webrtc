use anyhow::Result;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_H264, MIME_TYPE_VP8};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use std::sync::Arc;
use webrtc::error::Error;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder,
};
use webrtc::rtp_transceiver::{
    RTCRtpTransceiverDirection, RTCRtpTransceiverInit, RtpSender, RtpTransceiver,
};
use webrtc::runtime::block_on;

const BASE_SSRC: u32 = 0x1111_1111;
const ALT_SSRC: u32 = 0x2222_2222;
const LOW_RID: &str = "low";
const HIGH_RID: &str = "high";

#[derive(Clone)]
struct TestHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for TestHandler {}

fn video_codec(mime_type: &str, payload_type: u8) -> RTCRtpCodecParameters {
    RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: mime_type.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: String::new(),
            rtcp_feedback: vec![],
        },
        payload_type,
        ..Default::default()
    }
}

fn video_media_engine(codecs: &[RTCRtpCodecParameters]) -> MediaEngine {
    let mut media_engine = MediaEngine::default();
    for codec in codecs {
        media_engine
            .register_codec(codec.clone(), RtpCodecKind::Video)
            .expect("register codec");
    }
    media_engine
}

fn encoding(
    codec: &RTCRtpCodecParameters,
    ssrc: u32,
    rid: Option<&str>,
) -> RTCRtpEncodingParameters {
    RTCRtpEncodingParameters {
        rtp_coding_parameters: RTCRtpCodingParameters {
            rid: rid.unwrap_or_default().to_string(),
            ssrc: Some(ssrc),
            ..Default::default()
        },
        codec: codec.rtp_codec.clone(),
        ..Default::default()
    }
}

fn non_simulcast_encodings(
    vp8: &RTCRtpCodecParameters,
    h264: &RTCRtpCodecParameters,
) -> Vec<RTCRtpEncodingParameters> {
    vec![
        encoding(vp8, BASE_SSRC, None),
        encoding(h264, ALT_SSRC, None),
    ]
}

fn simulcast_encodings(codec: &RTCRtpCodecParameters) -> Vec<RTCRtpEncodingParameters> {
    vec![
        encoding(codec, BASE_SSRC, Some(LOW_RID)),
        encoding(codec, ALT_SSRC, Some(HIGH_RID)),
    ]
}

fn invalid_mixed_encodings(
    vp8: &RTCRtpCodecParameters,
    h264: &RTCRtpCodecParameters,
) -> Vec<RTCRtpEncodingParameters> {
    vec![
        encoding(vp8, BASE_SSRC, None),
        encoding(h264, ALT_SSRC, Some(HIGH_RID)),
    ]
}

fn video_track(encodings: Vec<RTCRtpEncodingParameters>) -> Arc<TrackLocalStaticRTP> {
    Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        "stream".to_string(),
        "video".to_string(),
        "video".to_string(),
        RtpCodecKind::Video,
        encodings,
    )))
}

fn assert_non_simulcast_offer(sdp: &str) {
    assert!(!sdp.contains("a=rid:"), "{sdp}");
    assert!(!sdp.contains("a=simulcast:"), "{sdp}");
    assert!(sdp.contains(&format!("a=ssrc:{BASE_SSRC}")), "{sdp}");
    assert!(!sdp.contains(&format!("a=ssrc:{ALT_SSRC}")), "{sdp}");
}

fn assert_non_simulcast_answer(sdp: &str) {
    assert!(!sdp.contains("a=rid:"), "{sdp}");
    assert!(!sdp.contains("a=simulcast:"), "{sdp}");
}

fn assert_simulcast_offer(sdp: &str) {
    assert!(sdp.contains("a=rid:low send"), "{sdp}");
    assert!(sdp.contains("a=rid:high send"), "{sdp}");
    assert!(sdp.contains("a=simulcast:send low;high"), "{sdp}");
}

fn assert_simulcast_answer(sdp: &str) {
    assert!(sdp.contains("a=rid:low recv"), "{sdp}");
    assert!(sdp.contains("a=rid:high recv"), "{sdp}");
    assert!(sdp.contains("a=simulcast:recv low;high"), "{sdp}");
}

async fn build_pc(
    config: rtc::peer_connection::configuration::RTCConfiguration,
    media_engine: MediaEngine,
) -> Result<Arc<dyn PeerConnection>> {
    Ok(Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_handler(Arc::new(TestHandler))
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await?,
    ))
}

async fn sender_from_transceiver(transceiver: &Arc<dyn RtpTransceiver>) -> Arc<dyn RtpSender> {
    transceiver
        .sender()
        .await
        .expect("sender result")
        .expect("sender should exist")
}

#[test]
fn test_add_transceiver_from_kind_negotiates_non_first_codec() {
    block_on(async {
        let vp8 = video_codec(MIME_TYPE_VP8, 96);
        let h264 = video_codec(MIME_TYPE_H264, 102);

        let config = RTCConfigurationBuilder::new().build();
        let offerer = build_pc(
            config.clone(),
            video_media_engine(&[vp8.clone(), h264.clone()]),
        )
        .await
        .unwrap();

        let transceiver = offerer
            .add_transceiver_from_kind(
                RtpCodecKind::Video,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    streams: vec![],
                    send_encodings: non_simulcast_encodings(&vp8, &h264),
                }),
            )
            .await
            .unwrap();

        let sender = sender_from_transceiver(&transceiver).await;
        let provisional_track = sender.track().track().await;
        assert_eq!(provisional_track.kind(), RtpCodecKind::Video);
        assert_eq!(provisional_track.codings().len(), 1);
        assert_eq!(
            provisional_track.codings()[0].rtp_coding_parameters.ssrc,
            Some(BASE_SSRC)
        );

        let offer = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer.clone()).await.unwrap();
        assert_non_simulcast_offer(&offer.sdp);

        let answerer = build_pc(config, video_media_engine(&[h264.clone()]))
            .await
            .unwrap();
        answerer.set_remote_description(offer).await.unwrap();
        let answer = answerer.create_answer(None).await.unwrap();
        assert!(answer.sdp.contains("H264/90000"), "{}", answer.sdp);
        assert!(!answer.sdp.contains("VP8/90000"), "{}", answer.sdp);
        assert_non_simulcast_answer(&answer.sdp);
        answerer
            .set_local_description(answer.clone())
            .await
            .unwrap();
        offerer.set_remote_description(answer).await.unwrap();

        let parameters = sender.get_parameters().await.unwrap();
        assert_eq!(parameters.rtp_parameters.codecs.len(), 1);
        assert_eq!(
            parameters.rtp_parameters.codecs[0].rtp_codec.mime_type,
            MIME_TYPE_H264
        );
        assert_eq!(parameters.encodings.len(), 1);
        assert_eq!(
            parameters.encodings[0].rtp_coding_parameters.ssrc,
            Some(BASE_SSRC)
        );

        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    })
}

#[test]
fn test_add_transceiver_from_track_negotiates_non_first_codec() {
    block_on(async {
        let vp8 = video_codec(MIME_TYPE_VP8, 96);
        let h264 = video_codec(MIME_TYPE_H264, 102);

        let config = RTCConfigurationBuilder::new().build();
        let offerer = build_pc(
            config.clone(),
            video_media_engine(&[vp8.clone(), h264.clone()]),
        )
        .await
        .unwrap();

        let track = video_track(non_simulcast_encodings(&vp8, &h264));
        let transceiver = offerer
            .add_transceiver_from_track(
                track,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    streams: vec![],
                    send_encodings: vec![],
                }),
            )
            .await
            .unwrap();

        let sender = sender_from_transceiver(&transceiver).await;
        let provisional_track = sender.track().track().await;
        assert_eq!(provisional_track.codings().len(), 2);
        assert_eq!(
            provisional_track.codings()[0].rtp_coding_parameters.ssrc,
            Some(BASE_SSRC)
        );

        let offer = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer.clone()).await.unwrap();
        assert_non_simulcast_offer(&offer.sdp);

        let answerer = build_pc(config, video_media_engine(&[h264.clone()]))
            .await
            .unwrap();
        answerer.set_remote_description(offer).await.unwrap();
        let answer = answerer.create_answer(None).await.unwrap();
        assert!(answer.sdp.contains("H264/90000"), "{}", answer.sdp);
        assert!(!answer.sdp.contains("VP8/90000"), "{}", answer.sdp);
        assert_non_simulcast_answer(&answer.sdp);
        answerer
            .set_local_description(answer.clone())
            .await
            .unwrap();
        offerer.set_remote_description(answer).await.unwrap();

        let parameters = sender.get_parameters().await.unwrap();
        assert_eq!(parameters.rtp_parameters.codecs.len(), 1);
        assert_eq!(
            parameters.rtp_parameters.codecs[0].rtp_codec.mime_type,
            MIME_TYPE_H264
        );
        assert_eq!(parameters.encodings.len(), 1);
        assert_eq!(
            parameters.encodings[0].rtp_coding_parameters.ssrc,
            Some(BASE_SSRC)
        );

        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    })
}

#[test]
fn test_add_track_negotiates_non_first_codec() {
    block_on(async {
        let vp8 = video_codec(MIME_TYPE_VP8, 96);
        let h264 = video_codec(MIME_TYPE_H264, 102);

        let config = RTCConfigurationBuilder::new().build();
        let offerer = build_pc(
            config.clone(),
            video_media_engine(&[vp8.clone(), h264.clone()]),
        )
        .await
        .unwrap();

        let sender = offerer
            .add_track(video_track(non_simulcast_encodings(&vp8, &h264)))
            .await
            .unwrap();

        let provisional_track = sender.track().track().await;
        assert_eq!(provisional_track.codings().len(), 2);
        assert_eq!(
            provisional_track.codings()[0].rtp_coding_parameters.ssrc,
            Some(BASE_SSRC)
        );

        let offer = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer.clone()).await.unwrap();
        assert_non_simulcast_offer(&offer.sdp);

        let answerer = build_pc(config, video_media_engine(&[h264.clone()]))
            .await
            .unwrap();
        answerer.set_remote_description(offer).await.unwrap();
        let answer = answerer.create_answer(None).await.unwrap();
        assert!(answer.sdp.contains("H264/90000"), "{}", answer.sdp);
        assert!(!answer.sdp.contains("VP8/90000"), "{}", answer.sdp);
        assert_non_simulcast_answer(&answer.sdp);
        answerer
            .set_local_description(answer.clone())
            .await
            .unwrap();
        offerer.set_remote_description(answer).await.unwrap();

        let parameters = sender.get_parameters().await.unwrap();
        assert_eq!(parameters.rtp_parameters.codecs.len(), 1);
        assert_eq!(
            parameters.rtp_parameters.codecs[0].rtp_codec.mime_type,
            MIME_TYPE_H264
        );
        assert_eq!(parameters.encodings.len(), 1);
        assert_eq!(
            parameters.encodings[0].rtp_coding_parameters.ssrc,
            Some(BASE_SSRC)
        );

        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    })
}

#[test]
fn test_add_transceiver_from_kind_preserves_simulcast() {
    block_on(async {
        let vp8 = video_codec(MIME_TYPE_VP8, 96);
        let config = RTCConfigurationBuilder::new().build();
        let offerer = build_pc(config.clone(), video_media_engine(&[vp8.clone()]))
            .await
            .unwrap();

        let transceiver = offerer
            .add_transceiver_from_kind(
                RtpCodecKind::Video,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    streams: vec![],
                    send_encodings: simulcast_encodings(&vp8),
                }),
            )
            .await
            .unwrap();

        let sender = sender_from_transceiver(&transceiver).await;
        assert_eq!(sender.track().track().await.codings().len(), 2);

        let offer = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer.clone()).await.unwrap();
        assert_simulcast_offer(&offer.sdp);

        let answerer = build_pc(config, video_media_engine(&[vp8.clone()]))
            .await
            .unwrap();
        answerer.set_remote_description(offer).await.unwrap();
        let answer = answerer.create_answer(None).await.unwrap();
        assert!(answer.sdp.contains("VP8/90000"), "{}", answer.sdp);
        assert_simulcast_answer(&answer.sdp);
        answerer
            .set_local_description(answer.clone())
            .await
            .unwrap();
        offerer.set_remote_description(answer).await.unwrap();

        let parameters = sender.get_parameters().await.unwrap();
        assert_eq!(parameters.rtp_parameters.codecs.len(), 1);
        assert_eq!(
            parameters.rtp_parameters.codecs[0].rtp_codec.mime_type,
            MIME_TYPE_VP8
        );
        assert_eq!(parameters.encodings.len(), 2);

        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    })
}

#[test]
fn test_add_transceiver_from_track_preserves_simulcast() {
    block_on(async {
        let vp8 = video_codec(MIME_TYPE_VP8, 96);
        let config = RTCConfigurationBuilder::new().build();
        let offerer = build_pc(config.clone(), video_media_engine(&[vp8.clone()]))
            .await
            .unwrap();

        let transceiver = offerer
            .add_transceiver_from_track(
                video_track(simulcast_encodings(&vp8)),
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    streams: vec![],
                    send_encodings: vec![],
                }),
            )
            .await
            .unwrap();

        let sender = sender_from_transceiver(&transceiver).await;
        assert_eq!(sender.track().track().await.codings().len(), 2);

        let offer = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer.clone()).await.unwrap();
        assert_simulcast_offer(&offer.sdp);

        let answerer = build_pc(config, video_media_engine(&[vp8.clone()]))
            .await
            .unwrap();
        answerer.set_remote_description(offer).await.unwrap();
        let answer = answerer.create_answer(None).await.unwrap();
        assert!(answer.sdp.contains("VP8/90000"), "{}", answer.sdp);
        assert_simulcast_answer(&answer.sdp);
        answerer
            .set_local_description(answer.clone())
            .await
            .unwrap();
        offerer.set_remote_description(answer).await.unwrap();

        let parameters = sender.get_parameters().await.unwrap();
        assert_eq!(parameters.rtp_parameters.codecs.len(), 1);
        assert_eq!(
            parameters.rtp_parameters.codecs[0].rtp_codec.mime_type,
            MIME_TYPE_VP8
        );
        assert_eq!(parameters.encodings.len(), 2);

        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    })
}

#[test]
fn test_add_track_preserves_simulcast() {
    block_on(async {
        let vp8 = video_codec(MIME_TYPE_VP8, 96);
        let config = RTCConfigurationBuilder::new().build();
        let offerer = build_pc(config.clone(), video_media_engine(&[vp8.clone()]))
            .await
            .unwrap();

        let sender = offerer
            .add_track(video_track(simulcast_encodings(&vp8)))
            .await
            .unwrap();

        assert_eq!(sender.track().track().await.codings().len(), 2);

        let offer = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer.clone()).await.unwrap();
        assert_simulcast_offer(&offer.sdp);

        let answerer = build_pc(config, video_media_engine(&[vp8.clone()]))
            .await
            .unwrap();
        answerer.set_remote_description(offer).await.unwrap();
        let answer = answerer.create_answer(None).await.unwrap();
        assert!(answer.sdp.contains("VP8/90000"), "{}", answer.sdp);
        assert_simulcast_answer(&answer.sdp);
        answerer
            .set_local_description(answer.clone())
            .await
            .unwrap();
        offerer.set_remote_description(answer).await.unwrap();

        let parameters = sender.get_parameters().await.unwrap();
        assert_eq!(parameters.rtp_parameters.codecs.len(), 1);
        assert_eq!(
            parameters.rtp_parameters.codecs[0].rtp_codec.mime_type,
            MIME_TYPE_VP8
        );
        assert_eq!(parameters.encodings.len(), 2);

        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    })
}

#[test]
fn test_add_transceiver_from_kind_rejects_invalid_rid_mix() {
    block_on(async {
        let vp8 = video_codec(MIME_TYPE_VP8, 96);
        let h264 = video_codec(MIME_TYPE_H264, 102);
        let config = RTCConfigurationBuilder::new().build();
        let offerer = build_pc(config, video_media_engine(&[vp8.clone(), h264.clone()]))
            .await
            .unwrap();

        let result = offerer
            .add_transceiver_from_kind(
                RtpCodecKind::Video,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    streams: vec![],
                    send_encodings: invalid_mixed_encodings(&vp8, &h264),
                }),
            )
            .await;

        assert!(matches!(result, Err(Error::ErrRTPSenderRidNil)));
        offerer.close().await.unwrap();
    })
}

#[test]
fn test_add_transceiver_from_track_rejects_invalid_rid_mix() {
    block_on(async {
        let vp8 = video_codec(MIME_TYPE_VP8, 96);
        let h264 = video_codec(MIME_TYPE_H264, 102);
        let config = RTCConfigurationBuilder::new().build();
        let offerer = build_pc(config, video_media_engine(&[vp8.clone(), h264.clone()]))
            .await
            .unwrap();

        let result = offerer
            .add_transceiver_from_track(
                video_track(invalid_mixed_encodings(&vp8, &h264)),
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    streams: vec![],
                    send_encodings: vec![],
                }),
            )
            .await;

        assert!(matches!(result, Err(Error::ErrRTPSenderRidNil)));
        offerer.close().await.unwrap();
    })
}

#[test]
fn test_add_track_rejects_invalid_rid_mix() {
    block_on(async {
        let vp8 = video_codec(MIME_TYPE_VP8, 96);
        let h264 = video_codec(MIME_TYPE_H264, 102);
        let config = RTCConfigurationBuilder::new().build();
        let offerer = build_pc(config, video_media_engine(&[vp8.clone(), h264.clone()]))
            .await
            .unwrap();

        let result = offerer
            .add_track(video_track(invalid_mixed_encodings(&vp8, &h264)))
            .await;

        assert!(matches!(result, Err(Error::ErrRTPSenderRidNil)));
        offerer.close().await.unwrap();
    })
}
