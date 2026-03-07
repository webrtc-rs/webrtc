use anyhow::Result;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_H264, MIME_TYPE_VP8};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use std::sync::Arc;
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCConfigurationBuilder,
};
use webrtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use webrtc::runtime::block_on;

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

#[test]
fn test_add_transceiver_from_kind_negotiates_non_first_codec() {
    block_on(async {
        run_test().await.unwrap();
    })
}

async fn run_test() -> Result<()> {
    let vp8 = video_codec(MIME_TYPE_VP8, 96);
    let h264 = video_codec(MIME_TYPE_H264, 102);

    let mut offerer_media_engine = MediaEngine::default();
    offerer_media_engine.register_codec(vp8.clone(), RtpCodecKind::Video)?;
    offerer_media_engine.register_codec(h264.clone(), RtpCodecKind::Video)?;

    let config = RTCConfigurationBuilder::new().build();
    let offerer = PeerConnectionBuilder::new()
        .with_configuration(config.clone())
        .with_media_engine(offerer_media_engine)
        .with_handler(Arc::new(TestHandler))
        .with_udp_addrs(vec!["127.0.0.1:0"])
        .build()
        .await?;

    let transceiver = offerer
        .add_transceiver_from_kind(
            RtpCodecKind::Video,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Sendonly,
                streams: vec![],
                send_encodings: vec![
                    RTCRtpEncodingParameters {
                        rtp_coding_parameters: RTCRtpCodingParameters {
                            ssrc: Some(0x1111_1111),
                            ..Default::default()
                        },
                        codec: vp8.rtp_codec.clone(),
                        ..Default::default()
                    },
                    RTCRtpEncodingParameters {
                        rtp_coding_parameters: RTCRtpCodingParameters {
                            ssrc: Some(0x2222_2222),
                            ..Default::default()
                        },
                        codec: h264.rtp_codec.clone(),
                        ..Default::default()
                    },
                ],
            }),
        )
        .await?;

    let sender = transceiver.sender().await?.expect("sender should exist");
    let provisional_track = sender.track().track().await;
    assert_eq!(provisional_track.kind(), RtpCodecKind::Video);
    assert_eq!(provisional_track.codings().len(), 2);

    let offer = offerer.create_offer(None).await?;
    offerer.set_local_description(offer.clone()).await?;

    let mut answerer_media_engine = MediaEngine::default();
    answerer_media_engine.register_codec(h264.clone(), RtpCodecKind::Video)?;

    let answerer = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(answerer_media_engine)
        .with_handler(Arc::new(TestHandler))
        .with_udp_addrs(vec!["127.0.0.1:0"])
        .build()
        .await?;

    answerer.set_remote_description(offer).await?;

    let answer = answerer.create_answer(None).await?;
    assert!(answer.sdp.contains("H264/90000"), "{}", answer.sdp);
    assert!(!answer.sdp.contains("VP8/90000"), "{}", answer.sdp);

    answerer.set_local_description(answer.clone()).await?;
    offerer.set_remote_description(answer).await?;

    let parameters = sender.get_parameters().await?;
    assert_eq!(parameters.rtp_parameters.codecs.len(), 1);
    assert_eq!(
        parameters.rtp_parameters.codecs[0].rtp_codec.mime_type,
        MIME_TYPE_H264
    );

    offerer.close().await?;
    answerer.close().await?;

    Ok(())
}
