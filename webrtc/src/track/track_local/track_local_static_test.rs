use super::{track_local_static_rtp::*, track_local_static_sample::*, *};
use crate::api::media_engine::{MediaEngine, MIME_TYPE_VP8};
use crate::api::APIBuilder;
use crate::peer_connection::configuration::RTCConfiguration;
use crate::peer_connection::peer_connection_test::*;

use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

// If a remote doesn't support a Codec used by a `TrackLocalStatic`
// an error should be returned to the user
#[tokio::test]
async fn test_track_local_static_no_codec_intersection() -> Result<()> {
    let track: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: "video/vp8".to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    //"Offerer"
    {
        let mut pc = api.new_peer_connection(RTCConfiguration::default()).await?;

        let mut no_codec_pc = APIBuilder::new()
            .build()
            .new_peer_connection(RTCConfiguration::default())
            .await?;

        pc.add_track(Arc::clone(&track)).await?;

        if let Err(err) = signal_pair(&mut pc, &mut no_codec_pc).await {
            assert_eq!(err, Error::ErrUnsupportedCodec);
        } else {
            panic!();
        }

        close_pair_now(&no_codec_pc, &pc).await;
    }

    //"Answerer"
    {
        let mut pc = api.new_peer_connection(RTCConfiguration::default()).await?;

        let mut m = MediaEngine::default();
        m.register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/VP9".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;
        let mut vp9only_pc = APIBuilder::new()
            .with_media_engine(m)
            .build()
            .new_peer_connection(RTCConfiguration::default())
            .await?;

        vp9only_pc
            .add_transceiver_from_kind(RTPCodecType::Video, None)
            .await?;

        pc.add_track(Arc::clone(&track)).await?;

        if let Err(err) = signal_pair(&mut vp9only_pc, &mut pc).await {
            assert_eq!(
                err,
                Error::ErrUnsupportedCodec,
                "expected {}, but got {}",
                Error::ErrUnsupportedCodec,
                err
            );
        } else {
            panic!();
        }

        close_pair_now(&vp9only_pc, &pc).await;
    }

    //"Local"
    {
        let (mut offerer, mut answerer) = new_pair(&api).await?;

        let invalid_codec_track = TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "video/invalid-codec".to_owned(),
                ..Default::default()
            },
            "video".to_owned(),
            "webrtc-rs".to_owned(),
        );

        offerer.add_track(Arc::new(invalid_codec_track)).await?;

        if let Err(err) = signal_pair(&mut offerer, &mut answerer).await {
            assert_eq!(err, Error::ErrUnsupportedCodec);
        } else {
            panic!();
        }

        close_pair_now(&offerer, &answerer).await;
    }

    Ok(())
}

// Assert that Bind/Unbind happens when expected
#[tokio::test]
async fn test_track_local_static_closed() -> Result<()> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut pc_offer, mut pc_answer) = new_pair(&api).await?;

    pc_answer
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    let vp8writer: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: "video/vp8".to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    pc_offer.add_track(Arc::clone(&vp8writer)).await?;

    if let Some(v) = vp8writer.as_any().downcast_ref::<TrackLocalStaticRTP>() {
        let bindings = v.bindings.lock().await;
        assert_eq!(
            bindings.len(),
            0,
            "No binding should exist before signaling"
        );
    } else {
        panic!();
    }

    signal_pair(&mut pc_offer, &mut pc_answer).await?;

    if let Some(v) = vp8writer.as_any().downcast_ref::<TrackLocalStaticRTP>() {
        let bindings = v.bindings.lock().await;
        assert_eq!(bindings.len(), 1, "binding should exist after signaling");
    } else {
        panic!();
    }

    close_pair_now(&pc_offer, &pc_answer).await;

    if let Some(v) = vp8writer.as_any().downcast_ref::<TrackLocalStaticRTP>() {
        let bindings = v.bindings.lock().await;
        assert_eq!(bindings.len(), 0, "No binding should exist after close");
    } else {
        panic!();
    }

    Ok(())
}

//use log::LevelFilter;
//use std::io::Write;

#[tokio::test]
async fn test_track_local_static_payload_type() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, LevelFilter::Trace)
    .init();*/

    let mut media_engine_one = MediaEngine::default();
    media_engine_one.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 100,
            ..Default::default()
        },
        RTPCodecType::Video,
    )?;

    let mut media_engine_two = MediaEngine::default();
    media_engine_two.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 200,
            ..Default::default()
        },
        RTPCodecType::Video,
    )?;

    let mut offerer = APIBuilder::new()
        .with_media_engine(media_engine_one)
        .build()
        .new_peer_connection(RTCConfiguration::default())
        .await?;
    let mut answerer = APIBuilder::new()
        .with_media_engine(media_engine_two)
        .build()
        .new_peer_connection(RTCConfiguration::default())
        .await?;

    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    offerer
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    answerer
        .add_track(Arc::clone(&track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let (on_track_fired_tx, on_track_fired_rx) = mpsc::channel::<()>(1);
    let on_track_fired_tx = Arc::new(Mutex::new(Some(on_track_fired_tx)));
    offerer.on_track(Box::new(move |track, _, _| {
        let on_track_fired_tx2 = Arc::clone(&on_track_fired_tx);
        Box::pin(async move {
            assert_eq!(track.payload_type(), 100);
            assert_eq!(track.codec().capability.mime_type, MIME_TYPE_VP8);
            {
                log::debug!("onTrackFiredFunc!!!");
                let mut done = on_track_fired_tx2.lock().await;
                done.take();
            }
        })
    }));

    signal_pair(&mut offerer, &mut answerer).await?;

    send_video_until_done(
        on_track_fired_rx,
        vec![track],
        Bytes::from_static(&[0x00]),
        None,
    )
    .await;

    close_pair_now(&offerer, &answerer).await;

    Ok(())
}

// Assert that writing to a Track doesn't modify the input
// Even though we can pass a pointer we shouldn't modify the incoming value
#[tokio::test]
async fn test_track_local_static_mutate_input() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, LevelFilter::Trace)
    .init();*/

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (mut pc_offer, mut pc_answer) = new_pair(&api).await?;

    let vp8writer: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    pc_offer.add_track(Arc::clone(&vp8writer)).await?;

    signal_pair(&mut pc_offer, &mut pc_answer).await?;

    let pkt = rtp::packet::Packet {
        header: rtp::header::Header {
            ssrc: 1,
            payload_type: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    if let Some(v) = vp8writer.as_any().downcast_ref::<TrackLocalStaticRTP>() {
        v.write_rtp(&pkt).await?;
    } else {
        panic!();
    }

    assert_eq!(pkt.header.ssrc, 1);
    assert_eq!(pkt.header.payload_type, 1);

    close_pair_now(&pc_offer, &pc_answer).await;

    Ok(())
}

//use std::io::Write;
//use log::LevelFilter;

// Assert that writing to a Track that has Binded (but not connected)
// does not block
#[tokio::test]
async fn test_track_local_static_binding_non_blocking() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, LevelFilter::Trace)
    .init();*/

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let api = APIBuilder::new().with_media_engine(m).build();

    let (pc_offer, pc_answer) = new_pair(&api).await?;

    pc_offer
        .add_transceiver_from_kind(RTPCodecType::Video, None)
        .await?;

    let vp8writer: Arc<dyn TrackLocal + Send + Sync> = Arc::new(TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    pc_answer.add_track(Arc::clone(&vp8writer)).await?;

    let offer = pc_offer.create_offer(None).await?;
    pc_answer.set_remote_description(offer).await?;

    let answer = pc_answer.create_answer(None).await?;
    pc_answer.set_local_description(answer).await?;

    if let Some(v) = vp8writer.as_any().downcast_ref::<TrackLocalStaticRTP>() {
        v.write(&[0u8; 20]).await?;
    } else {
        panic!();
    }

    close_pair_now(&pc_offer, &pc_answer).await;

    Ok(())
}

/*
//TODO: func BenchmarkTrackLocalWrite(b *testing.B) {
    offerPC, answerPC, err := newPair()
    defer closePairNow(b, offerPC, answerPC)
    if err != nil {
        b.Fatalf("Failed to create a PC pair for testing")
    }

    track, err := NewTrackLocalStaticRTP(RTPCodecCapability{mime_type: MIME_TYPE_VP8}, "video", "pion")
    assert.NoError(b, err)

    _, err = offerPC.AddTrack(track)
    assert.NoError(b, err)

    _, err = answerPC.AddTransceiverFromKind(RTPCodecTypeVideo)
    assert.NoError(b, err)

    b.SetBytes(1024)

    buf := make([]byte, 1024)
    for i := 0; i < b.N; i++ {
        _, err := track.Write(buf)
        assert.NoError(b, err)
    }
}
*/
