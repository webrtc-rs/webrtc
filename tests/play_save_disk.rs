//! Integration test for streaming media from IVF/OGG files.
//!
//! This test establishes a PeerConnection, opens sample IVF (VP8) and Ogg (Opus) files,
//! writes their samples to TrackLocalStaticSample instances, and verifies that the
//! receiver successfully receives and parses the video and audio RTP packets.

use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use rtc::media::Sample;
use rtc::media::io::ivf_reader::IVFReader;
use rtc::media::io::ogg_reader::OggReader;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MIME_TYPE_VP8};
use rtc::rtp_transceiver::PayloadType;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use webrtc::media_stream::Track;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCIceGatheringState, RTCPeerConnectionState,
};
use webrtc::runtime::{block_on, channel, default_runtime, interval, timeout};

// ── Event Handlers ────────────────────────────────────────────────────────────

struct OffererHandler {
    gather_complete_tx: webrtc::runtime::Sender<()>,
    connected_tx: webrtc::runtime::Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for OffererHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
}

struct AnswererHandler {
    gather_complete_tx: webrtc::runtime::Sender<()>,
    connected_tx: webrtc::runtime::Sender<()>,
    video_packets_rx_count: Arc<AtomicU32>,
    audio_packets_rx_count: Arc<AtomicU32>,
    runtime: Arc<dyn webrtc::runtime::Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for AnswererHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let video_count = self.video_packets_rx_count.clone();
        let audio_count = self.audio_packets_rx_count.clone();
        let kind = track.kind().await;

        self.runtime.spawn(Box::pin(async move {
            while let Some(evt) = track.poll().await {
                if let TrackRemoteEvent::OnRtpPacket(packet) = evt {
                    match kind {
                        RtpCodecKind::Video => {
                            video_count.fetch_add(1, Ordering::SeqCst);
                        }
                        RtpCodecKind::Audio => {
                            audio_count.fetch_add(1, Ordering::SeqCst);
                        }
                        _ => {}
                    }
                    if packet.payload.is_empty() {
                        break;
                    }
                }
            }
        }));
    }
}

// ── Streaming Helpers ──────────────────────────────────────────────────────────

async fn stream_video(
    video_file_name: &str,
    video_track: Arc<TrackLocalStaticSample>,
    payload_type: PayloadType,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(video_file_name)?;
    let reader = BufReader::new(file);
    let (mut ivf, header) = IVFReader::new(reader)?;

    let ssrc = *video_track.ssrcs().await.first().ok_or("no video ssrc")?;

    let sleep_time = Duration::from_millis(
        ((1000 * header.timebase_numerator) / header.timebase_denominator) as u64,
    );
    let mut ticker = interval(sleep_time);

    // Stream up to 50 frames
    for _ in 0..50 {
        let frame = match ivf.parse_next_frame() {
            Ok((frame, _)) => frame,
            Err(_) => break,
        };

        video_track
            .sample_writer(ssrc, payload_type)
            .write_sample(&Sample {
                data: frame.freeze(),
                duration: Duration::from_secs(1),
                ..Default::default()
            })
            .await?;

        let _ = ticker.tick().await;
    }

    Ok(())
}

async fn stream_audio(
    audio_file_name: &str,
    audio_track: Arc<TrackLocalStaticSample>,
    payload_type: PayloadType,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(audio_file_name)?;
    let reader = BufReader::new(file);
    let (mut ogg, _) = OggReader::new(reader, true)?;

    let ssrc = *audio_track.ssrcs().await.first().ok_or("no audio ssrc")?;

    let mut ticker = interval(Duration::from_millis(20));
    let mut last_granule: u64 = 0;

    // Stream up to 50 pages
    for _ in 0..50 {
        let (page_data, page_header) = match ogg.parse_next_page() {
            Ok(tup) => tup,
            Err(_) => break,
        };

        let sample_count = page_header.granule_position.saturating_sub(last_granule);
        last_granule = page_header.granule_position;
        let sample_duration = Duration::from_millis(sample_count * 1000 / 48000);

        audio_track
            .sample_writer(ssrc, payload_type)
            .write_sample(&Sample {
                data: page_data.freeze(),
                duration: sample_duration,
                ..Default::default()
            })
            .await?;

        let _ = ticker.tick().await;
    }

    Ok(())
}

// ── Test Case ─────────────────────────────────────────────────────────────────

#[test]
fn test_play_from_disk_streaming() {
    block_on(async {
        let runtime = default_runtime().expect("no runtime");

        let video_file = "rtc/examples/examples/test-data/output_vp8.ivf";
        let audio_file = "rtc/examples/examples/test-data/output.ogg";

        // Setup MediaEngines
        let mut offerer_media = MediaEngine::default();
        offerer_media.register_default_codecs().unwrap();
        let mut answerer_media = MediaEngine::default();
        answerer_media.register_default_codecs().unwrap();

        // Add Offer tracks using TrackLocalStaticSample
        let video_ssrc = rand::random::<u32>();
        let video_track = Arc::new(
            TrackLocalStaticSample::new(MediaStreamTrack::new(
                "video-stream".to_owned(),
                "video-track".to_owned(),
                "video-label".to_owned(),
                RtpCodecKind::Video,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(video_ssrc),
                        ..Default::default()
                    },
                    codec: RTCRtpCodec {
                        mime_type: MIME_TYPE_VP8.to_owned(),
                        clock_rate: 90000,
                        channels: 0,
                        sdp_fmtp_line: String::new(),
                        rtcp_feedback: vec![],
                    },
                    ..Default::default()
                }],
            ))
            .unwrap(),
        );

        let audio_ssrc = rand::random::<u32>();
        let audio_track = Arc::new(
            TrackLocalStaticSample::new(MediaStreamTrack::new(
                "audio-stream".to_owned(),
                "audio-track".to_owned(),
                "audio-label".to_owned(),
                RtpCodecKind::Audio,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(audio_ssrc),
                        ..Default::default()
                    },
                    codec: RTCRtpCodec {
                        mime_type: MIME_TYPE_OPUS.to_owned(),
                        clock_rate: 48000,
                        channels: 2,
                        sdp_fmtp_line: String::new(),
                        rtcp_feedback: vec![],
                    },
                    ..Default::default()
                }],
            ))
            .unwrap(),
        );

        // Build Offerer PeerConnection
        let (off_gather_tx, mut off_gather_rx) = channel(1);
        let (off_conn_tx, mut off_conn_rx) = channel(1);
        let offerer = PeerConnectionBuilder::new()
            .with_media_engine(offerer_media)
            .with_handler(Arc::new(OffererHandler {
                gather_complete_tx: off_gather_tx,
                connected_tx: off_conn_tx,
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .unwrap();
        let offerer = Arc::new(offerer);

        let video_sender = offerer
            .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal>)
            .await
            .unwrap();
        let audio_sender = offerer
            .add_track(Arc::clone(&audio_track) as Arc<dyn TrackLocal>)
            .await
            .unwrap();

        // Build Answerer PeerConnection
        let video_rx_count = Arc::new(AtomicU32::new(0));
        let audio_rx_count = Arc::new(AtomicU32::new(0));

        let (ans_gather_tx, mut ans_gather_rx) = channel(1);
        let (ans_conn_tx, mut ans_conn_rx) = channel(1);
        let answerer = PeerConnectionBuilder::new()
            .with_media_engine(answerer_media)
            .with_handler(Arc::new(AnswererHandler {
                gather_complete_tx: ans_gather_tx,
                connected_tx: ans_conn_tx,
                video_packets_rx_count: video_rx_count.clone(),
                audio_packets_rx_count: audio_rx_count.clone(),
                runtime: runtime.clone(),
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .unwrap();
        let answerer = Arc::new(answerer);

        // 1. Offerer create offer and set local description
        let offer = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer).await.unwrap();

        // 2. Wait for offerer ICE gathering
        timeout(Duration::from_secs(5), off_gather_rx.recv())
            .await
            .unwrap();
        let offer_sdp = offerer.local_description().await.unwrap();

        // 3. Answerer set remote description
        answerer.set_remote_description(offer_sdp).await.unwrap();

        // 4. Answerer create answer and set local description
        let answer = answerer.create_answer(None).await.unwrap();
        answerer.set_local_description(answer).await.unwrap();

        // 5. Wait for answerer ICE gathering
        timeout(Duration::from_secs(5), ans_gather_rx.recv())
            .await
            .unwrap();
        let answer_sdp = answerer.local_description().await.unwrap();

        // 6. Offerer set remote description
        offerer.set_remote_description(answer_sdp).await.unwrap();

        // Wait for connection to establish
        timeout(Duration::from_secs(5), off_conn_rx.recv())
            .await
            .unwrap();
        timeout(Duration::from_secs(5), ans_conn_rx.recv())
            .await
            .unwrap();

        // Spawn streaming tasks. write_sample stamps the payload type, and rtc's write_rtp
        // requires it to match a negotiated sender codec, so resolve it from the sender.
        let video_pt = video_sender
            .get_parameters()
            .await
            .unwrap()
            .rtp_parameters
            .codecs
            .first()
            .unwrap()
            .payload_type;
        let audio_pt = audio_sender
            .get_parameters()
            .await
            .unwrap()
            .rtp_parameters
            .codecs
            .first()
            .unwrap()
            .payload_type;
        let video_track_clone = video_track.clone();
        let audio_track_clone = audio_track.clone();
        runtime.spawn(Box::pin(async move {
            let _ = stream_video(video_file, video_track_clone, video_pt).await;
        }));
        runtime.spawn(Box::pin(async move {
            let _ = stream_audio(audio_file, audio_track_clone, audio_pt).await;
        }));

        // Wait up to 10 seconds for at least 30 packets to arrive on both streams
        let mut check_interval = interval(Duration::from_millis(100));
        let start_time = Instant::now();
        loop {
            let v_cnt = video_rx_count.load(Ordering::SeqCst);
            let a_cnt = audio_rx_count.load(Ordering::SeqCst);
            if v_cnt >= 30 && a_cnt >= 30 {
                break;
            }
            if start_time.elapsed() > Duration::from_secs(10) {
                panic!("Timeout: only received {v_cnt} video and {a_cnt} audio packets");
            }
            let _ = check_interval.tick().await;
        }

        // Close peer connections
        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    });
}
