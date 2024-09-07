use async_trait::async_trait;
use bytes::Bytes;
use interceptor::registry::Registry;
use interceptor::InterceptorBuilder;
use portable_atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::Duration;
use waitgroup::WaitGroup;

use super::*;
use crate::api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9};
use crate::api::setting_engine::SettingEngine;
use crate::api::APIBuilder;
use crate::error::Result;
use crate::peer_connection::peer_connection_state::RTCPeerConnectionState;
use crate::peer_connection::peer_connection_test::{
    close_pair_now, create_vnet_pair, new_pair, send_video_until_done, signal_pair,
    until_connection_state,
};
use crate::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use crate::rtp_transceiver::RTCRtpCodecParameters;
use crate::track::track_local::track_local_static_sample::TrackLocalStaticSample;

#[tokio::test]
async fn test_rtp_sender_replace_track() -> Result<()> {
    let mut s = SettingEngine::default();
    s.disable_srtp_replay_protection(true);

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;

    let api = APIBuilder::new()
        .with_setting_engine(s)
        .with_media_engine(m)
        .build();

    let (mut sender, mut receiver) = new_pair(&api).await?;

    let track_a = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let track_b = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let rtp_sender = sender
        .add_track(Arc::clone(&track_a) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let (seen_packet_a_tx, seen_packet_a_rx) = mpsc::channel::<()>(1);
    let (seen_packet_b_tx, seen_packet_b_rx) = mpsc::channel::<()>(1);

    let seen_packet_a_tx = Arc::new(seen_packet_a_tx);
    let seen_packet_b_tx = Arc::new(seen_packet_b_tx);
    let on_track_count = Arc::new(AtomicU64::new(0));
    receiver.on_track(Box::new(move |track, _, _| {
        assert_eq!(on_track_count.fetch_add(1, Ordering::SeqCst), 0);
        let seen_packet_a_tx2 = Arc::clone(&seen_packet_a_tx);
        let seen_packet_b_tx2 = Arc::clone(&seen_packet_b_tx);
        Box::pin(async move {
            let pkt = match track.read_rtp().await {
                Ok((pkt, _)) => pkt,
                Err(err) => {
                    //assert!(errors.Is(io.EOF, err))
                    log::debug!("{}", err);
                    return;
                }
            };

            let last = pkt.payload[pkt.payload.len() - 1];
            if last == 0xAA {
                assert_eq!(track.codec().capability.mime_type, MIME_TYPE_VP8);
                let _ = seen_packet_a_tx2.send(()).await;
            } else if last == 0xBB {
                assert_eq!(track.codec().capability.mime_type, MIME_TYPE_H264);
                let _ = seen_packet_b_tx2.send(()).await;
            } else {
                panic!("Unexpected RTP Data {last:02x}");
            }
        })
    }));

    signal_pair(&mut sender, &mut receiver).await?;

    // Block Until packet with 0xAA has been seen
    tokio::spawn(async move {
        send_video_until_done(
            seen_packet_a_rx,
            vec![track_a],
            Bytes::from_static(&[0xAA]),
            None,
        )
        .await;
    });

    rtp_sender
        .replace_track(Some(
            Arc::clone(&track_b) as Arc<dyn TrackLocal + Send + Sync>
        ))
        .await?;

    // Block Until packet with 0xBB has been seen
    tokio::spawn(async move {
        send_video_until_done(
            seen_packet_b_rx,
            vec![track_b],
            Bytes::from_static(&[0xBB]),
            None,
        )
        .await;
    });

    close_pair_now(&sender, &receiver).await;
    Ok(())
}

#[tokio::test]
async fn test_rtp_sender_get_parameters() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut offerer, mut answerer) = new_pair(&api).await?;

    let rtp_transceiver = offerer
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    signal_pair(&mut offerer, &mut answerer).await?;

    let sender = rtp_transceiver.sender().await;
    assert!(sender.track().await.is_some());
    let parameters = sender.get_parameters().await;
    assert_ne!(0, parameters.rtp_parameters.codecs.len());
    assert_eq!(1, parameters.encodings.len());
    assert_eq!(
        sender.track_encodings.lock().await[0].ssrc,
        parameters.encodings[0].ssrc
    );
    assert!(parameters.encodings[0].rid.is_empty());

    close_pair_now(&offerer, &answerer).await;
    Ok(())
}

#[tokio::test]
async fn test_rtp_sender_get_parameters_with_rid() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut offerer, mut answerer) = new_pair(&api).await?;

    let rtp_transceiver = offerer
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    signal_pair(&mut offerer, &mut answerer).await?;

    let rid = "moo";
    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        rid.to_owned(),
        "webrtc-rs".to_owned(),
    ));
    rtp_transceiver.set_sending_track(Some(track)).await?;

    let sender = rtp_transceiver.sender().await;
    assert!(sender.track().await.is_some());
    let parameters = sender.get_parameters().await;
    assert_ne!(0, parameters.rtp_parameters.codecs.len());
    assert_eq!(1, parameters.encodings.len());
    assert_eq!(
        sender.track_encodings.lock().await[0].ssrc,
        parameters.encodings[0].ssrc
    );
    assert_eq!(rid, parameters.encodings[0].rid);

    close_pair_now(&offerer, &answerer).await;
    Ok(())
}

#[tokio::test]
async fn test_rtp_sender_set_read_deadline() -> Result<()> {
    let (mut sender, mut receiver, wan) = create_vnet_pair().await?;

    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let rtp_sender = sender
        .add_track(Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let peer_connections_connected = WaitGroup::new();
    until_connection_state(
        &mut sender,
        &peer_connections_connected,
        RTCPeerConnectionState::Connected,
    )
    .await;
    until_connection_state(
        &mut receiver,
        &peer_connections_connected,
        RTCPeerConnectionState::Connected,
    )
    .await;

    signal_pair(&mut sender, &mut receiver).await?;

    peer_connections_connected.wait().await;

    let result = tokio::time::timeout(Duration::from_secs(1), rtp_sender.read_rtcp()).await;
    assert!(result.is_err());

    {
        let mut w = wan.lock().await;
        w.stop().await?;
    }
    close_pair_now(&sender, &receiver).await;

    Ok(())
}

#[tokio::test]
async fn test_rtp_sender_replace_track_invalid_track_kind_change() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut sender, mut receiver) = new_pair(&api).await?;

    let track_a = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let track_b = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            ..Default::default()
        },
        "audio".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let rtp_sender = sender
        .add_track(Arc::clone(&track_a) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    signal_pair(&mut sender, &mut receiver).await?;

    let (seen_packet_tx, seen_packet_rx) = mpsc::channel::<()>(1);
    let seen_packet_tx = Arc::new(seen_packet_tx);
    receiver.on_track(Box::new(move |_, _, _| {
        let seen_packet_tx2 = Arc::clone(&seen_packet_tx);
        Box::pin(async move {
            let _ = seen_packet_tx2.send(()).await;
        })
    }));

    tokio::spawn(async move {
        send_video_until_done(
            seen_packet_rx,
            vec![track_a],
            Bytes::from_static(&[0xAA]),
            None,
        )
        .await;
    });

    if let Err(err) = rtp_sender.replace_track(Some(track_b)).await {
        assert_eq!(err, Error::ErrRTPSenderNewTrackHasIncorrectKind);
    } else {
        panic!();
    }

    close_pair_now(&sender, &receiver).await;
    Ok(())
}

#[tokio::test]
async fn test_rtp_sender_replace_track_invalid_codec_change() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut sender, mut receiver) = new_pair(&api).await?;

    let track_a = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let track_b = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP9.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let rtp_sender = sender
        .add_track(Arc::clone(&track_a) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    {
        let tr = rtp_sender.rtp_transceiver.lock();
        let t = tr
            .as_ref()
            .and_then(|t| t.upgrade())
            .expect("Weak transceiver valid");
        t.set_codec_preferences(vec![RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                ..Default::default()
            },
            payload_type: 96,
            ..Default::default()
        }])
        .await?;
    }

    signal_pair(&mut sender, &mut receiver).await?;

    let (seen_packet_tx, seen_packet_rx) = mpsc::channel::<()>(1);
    let seen_packet_tx = Arc::new(seen_packet_tx);
    receiver.on_track(Box::new(move |_, _, _| {
        let seen_packet_tx2 = Arc::clone(&seen_packet_tx);
        Box::pin(async move {
            let _ = seen_packet_tx2.send(()).await;
        })
    }));

    tokio::spawn(async move {
        send_video_until_done(
            seen_packet_rx,
            vec![track_a],
            Bytes::from_static(&[0xAA]),
            None,
        )
        .await;
    });

    if let Err(err) = rtp_sender.replace_track(Some(track_b)).await {
        assert_eq!(err, Error::ErrUnsupportedCodec);
    } else {
        panic!();
    }

    close_pair_now(&sender, &receiver).await;
    Ok(())
}

#[tokio::test]
async fn test_rtp_sender_get_parameters_replaced() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (sender, receiver) = new_pair(&api).await?;
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    let rtp_sender = sender.add_track(track).await?;
    let param = rtp_sender.get_parameters().await;
    assert_eq!(1, param.encodings.len());

    rtp_sender.replace_track(None).await?;
    let param = rtp_sender.get_parameters().await;
    assert_eq!(0, param.encodings.len());

    close_pair_now(&sender, &receiver).await;
    Ok(())
}

#[tokio::test]
async fn test_rtp_sender_send() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (sender, receiver) = new_pair(&api).await?;
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    let rtp_sender = sender.add_track(track).await?;
    let param = rtp_sender.get_parameters().await;
    assert_eq!(1, param.encodings.len());

    rtp_sender.send(&param).await?;

    assert_eq!(
        Error::ErrRTPSenderSendAlreadyCalled,
        rtp_sender.send(&param).await.unwrap_err()
    );

    close_pair_now(&sender, &receiver).await;
    Ok(())
}

#[tokio::test]
async fn test_rtp_sender_send_track_removed() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (sender, receiver) = new_pair(&api).await?;
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    let rtp_sender = sender.add_track(track).await?;
    let param = rtp_sender.get_parameters().await;
    assert_eq!(1, param.encodings.len());

    sender.remove_track(&rtp_sender).await?;
    assert_eq!(
        Error::ErrRTPSenderTrackRemoved,
        rtp_sender.send(&param).await.unwrap_err()
    );

    close_pair_now(&sender, &receiver).await;
    Ok(())
}

#[tokio::test]
async fn test_rtp_sender_add_encoding() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (sender, receiver) = new_pair(&api).await?;
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    let rtp_sender = sender.add_track(track).await?;

    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    assert_eq!(
        Error::ErrRTPSenderRidNil,
        rtp_sender.add_encoding(track).await.unwrap_err()
    );

    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "h".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    assert_eq!(
        Error::ErrRTPSenderNoBaseEncoding,
        rtp_sender.add_encoding(track).await.unwrap_err()
    );

    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "f".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    let rtp_sender = sender.add_track(track).await?;

    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video-foobar".to_owned(),
        "h".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    assert_eq!(
        Error::ErrRTPSenderBaseEncodingMismatch,
        rtp_sender.add_encoding(track).await.unwrap_err()
    );

    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "h".to_owned(),
        "webrtc-rs-foobar".to_owned(),
    ));
    assert_eq!(
        Error::ErrRTPSenderBaseEncodingMismatch,
        rtp_sender.add_encoding(track).await.unwrap_err()
    );

    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "h".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    assert_eq!(
        Error::ErrRTPSenderBaseEncodingMismatch,
        rtp_sender.add_encoding(track).await.unwrap_err()
    );

    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "f".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    assert_eq!(
        Error::ErrRTPSenderRIDCollision,
        rtp_sender.add_encoding(track).await.unwrap_err()
    );

    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "h".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    rtp_sender.add_encoding(track).await?;

    rtp_sender.send(&rtp_sender.get_parameters().await).await?;

    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "f".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    assert_eq!(
        Error::ErrRTPSenderSendAlreadyCalled,
        rtp_sender.add_encoding(track).await.unwrap_err()
    );

    rtp_sender.stop().await?;

    let track = Arc::new(TrackLocalStaticSample::new_with_rid(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "f".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    assert_eq!(
        Error::ErrRTPSenderStopped,
        rtp_sender.add_encoding(track).await.unwrap_err()
    );

    close_pair_now(&sender, &receiver).await;
    Ok(())
}

#[derive(Debug)]
enum TestInterceptorEvent {
    BindLocal(StreamInfo),
    BindRemote(StreamInfo),
    UnbindLocal(StreamInfo),
    UnbindRemote(StreamInfo),
}

#[derive(Clone)]
struct TestInterceptor(mpsc::UnboundedSender<TestInterceptorEvent>);

#[async_trait]
impl Interceptor for TestInterceptor {
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        reader
    }

    async fn bind_rtcp_writer(
        &self,
        writer: Arc<dyn interceptor::RTCPWriter + Send + Sync>,
    ) -> Arc<dyn interceptor::RTCPWriter + Send + Sync> {
        writer
    }

    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        let _ = self.0.send(TestInterceptorEvent::BindLocal(info.clone()));
        writer
    }

    async fn unbind_local_stream(&self, info: &StreamInfo) {
        let _ = self.0.send(TestInterceptorEvent::UnbindLocal(info.clone()));
    }

    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        reader: Arc<dyn interceptor::RTPReader + Send + Sync>,
    ) -> Arc<dyn interceptor::RTPReader + Send + Sync> {
        let _ = self.0.send(TestInterceptorEvent::BindRemote(info.clone()));
        reader
    }

    async fn unbind_remote_stream(&self, info: &StreamInfo) {
        let _ = self
            .0
            .send(TestInterceptorEvent::UnbindRemote(info.clone()));
    }

    async fn close(&self) -> std::result::Result<(), interceptor::Error> {
        Ok(())
    }
}

impl InterceptorBuilder for TestInterceptor {
    fn build(
        &self,
        _id: &str,
    ) -> std::result::Result<Arc<dyn Interceptor + Send + Sync>, interceptor::Error> {
        Ok(Arc::new(self.clone()))
    }
}

#[tokio::test]
async fn test_rtp_sender_rtx() -> Result<()> {
    let mut s = SettingEngine::default();
    s.enable_sender_rtx(true);

    let (interceptor_tx, mut interceptor_rx) = mpsc::unbounded_channel();

    let mut registry = Registry::new();
    registry.add(Box::new(TestInterceptor(interceptor_tx)));

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    // only register rtx for VP8
    m.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
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

    let api = APIBuilder::new()
        .with_setting_engine(s)
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    let (mut offerer, mut answerer) = new_pair(&api).await?;

    let track_a = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let track_b = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let rtp_sender_a = offerer
        .add_track(Arc::clone(&track_a) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let rtp_sender_b = offerer
        .add_track(Arc::clone(&track_b) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    signal_pair(&mut offerer, &mut answerer).await?;

    // rtx enabled for vp8
    assert!(rtp_sender_a.track().await.is_some());
    assert!(rtp_sender_a.track_encodings.lock().await[0].rtx.is_some());

    // no rtx for h264
    assert!(rtp_sender_b.track().await.is_some());
    assert!(rtp_sender_b.track_encodings.lock().await[0].rtx.is_some());

    close_pair_now(&offerer, &answerer).await;

    let mut vp8_ssrcs = Vec::new();
    let mut h264_ssrcs = Vec::new();
    let mut rtx_associated_ssrcs = Vec::new();

    // pair closed, all interceptor events should be buffered
    while let Ok(event) = interceptor_rx.try_recv() {
        if let TestInterceptorEvent::BindLocal(info) = event {
            match info.mime_type.as_str() {
                MIME_TYPE_VP8 => vp8_ssrcs.push(info.ssrc),
                MIME_TYPE_H264 => h264_ssrcs.push(info.ssrc),
                "video/rtx" => rtx_associated_ssrcs.push(
                    info.associated_stream
                        .expect("rtx without asscoiated stream")
                        .ssrc,
                ),
                mime => panic!("unexpected mime: {mime}"),
            }
        }
    }

    assert_eq!(vp8_ssrcs.len(), 1);
    assert_eq!(h264_ssrcs.len(), 1);
    assert_eq!(rtx_associated_ssrcs, vp8_ssrcs);

    Ok(())
}
