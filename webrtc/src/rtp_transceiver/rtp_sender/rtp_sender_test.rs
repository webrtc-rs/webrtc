use std::sync::atomic::AtomicU64;

use bytes::Bytes;
use tokio::time::Duration;
use waitgroup::WaitGroup;

use super::*;
use crate::api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9};
use crate::api::setting_engine::SettingEngine;
use crate::api::APIBuilder;
use crate::error::Result;
use crate::peer_connection::peer_connection_state::RTCPeerConnectionState;
use crate::peer_connection::peer_connection_test::{
    close_pair_now, create_vnet_pair, new_pair, send_video_until_done, signal_pair, StateHandler,
};
use crate::peer_connection::PeerConnectionEventHandler;
use crate::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use crate::rtp_transceiver::{RTCRtpReceiver, RTCRtpTransceiver};

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

    struct TrackHandler {
        seen_packet_a: Arc<mpsc::Sender<()>>,
        seen_packet_b: Arc<mpsc::Sender<()>>,
        track_count: Arc<AtomicU64>,
    }

    impl PeerConnectionEventHandler for TrackHandler {
        fn on_track(
            &mut self,
            track: Arc<crate::track::track_remote::TrackRemote>,
            _: Arc<RTCRtpReceiver>,
            _: Arc<RTCRtpTransceiver>,
        ) -> impl std::future::Future<Output = ()> + Send {
            async {
                assert_eq!(self.track_count.fetch_add(1, Ordering::SeqCst), 0);
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
                    let _ = self.seen_packet_a.send(()).await;
                } else if last == 0xBB {
                    assert_eq!(track.codec().capability.mime_type, MIME_TYPE_H264);
                    let _ = self.seen_packet_b.send(()).await;
                } else {
                    panic!("Unexpected RTP Data {last:02x}");
                }
            }
        }
    }
    receiver.with_event_handler(TrackHandler {
        seen_packet_a,
        seen_packet_b,
        track_count: on_track_count.clone(),
    });

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
    let parameters = sender.get_parameters().await;
    assert_ne!(0, parameters.rtp_parameters.codecs.len());
    assert_eq!(1, parameters.encodings.len());
    assert_eq!(sender.ssrc, parameters.encodings[0].ssrc);

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
    sender.with_event_handler(StateHandler {
        worker: Arc::new(Mutex::new(Some(peer_connections_connected.worker()))),
    });
    receiver.with_event_handler(StateHandler {
        worker: Arc::new(Mutex::new(Some(peer_connections_connected.worker()))),
    });

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

struct TrackPacketHandler {
    seen_packet_tx: Arc<mpsc::Sender<()>>,
}

impl PeerConnectionEventHandler for TrackPacketHandler {
    fn on_track(
        &mut self,
        _: Arc<crate::track::track_remote::TrackRemote>,
        _: Arc<RTCRtpReceiver>,
        _: Arc<RTCRtpTransceiver>,
    ) -> impl std::future::Future<Output = ()> + Send {
        async { self.seen_packet_tx.send(()).await }
    }
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

    receiver.with_event_handler(TrackPacketHandler { seen_packet_tx });

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
        if let Some(t) = &*tr {
            if let Some(t) = t.upgrade() {
                t.set_codec_preferences(vec![RTCRtpCodecParameters {
                    capability: RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_VP8.to_owned(),
                        ..Default::default()
                    },
                    payload_type: 96,
                    ..Default::default()
                }])
                .await?;
            } else {
                panic!();
            }
        } else {
            panic!();
        }
    }

    signal_pair(&mut sender, &mut receiver).await?;

    let (seen_packet_tx, seen_packet_rx) = mpsc::channel::<()>(1);
    let seen_packet_tx = Arc::new(seen_packet_tx);

    receiver.with_event_handler(TrackPacketHandler { seen_packet_tx });

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
