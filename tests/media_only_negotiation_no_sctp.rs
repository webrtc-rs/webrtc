//! Integration test for media-only async WebRTC negotiation without SCTP.
//!
//! This is the async WebRTC counterpart to the rtc-side regression test for
//! commit 897422b8:
//! "Only start SCTP transport if application media has been negotiated".
//!
//! Test scenario:
//! - Offerer negotiates a single video track only
//! - Neither SDP contains an `m=application` section
//! - RTP flows successfully between the async peers
//! - No data-channel callbacks are observed

use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::MIME_TYPE_VP8;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCIceGatheringState, RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

struct OffererHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
    unexpected_data_channels: Arc<AtomicU32>,
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

    async fn on_data_channel(&self, _dc: Arc<dyn webrtc::data_channel::DataChannel>) {
        self.unexpected_data_channels.fetch_add(1, Ordering::SeqCst);
    }
}

struct AnswererHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
    unexpected_data_channels: Arc<AtomicU32>,
    track_open_count: Arc<AtomicU32>,
    rtp_packets_received: Arc<AtomicU32>,
    runtime: Arc<dyn Runtime>,
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

    async fn on_data_channel(&self, _dc: Arc<dyn webrtc::data_channel::DataChannel>) {
        self.unexpected_data_channels.fetch_add(1, Ordering::SeqCst);
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let track_open_count = self.track_open_count.clone();
        let rtp_packets_received = self.rtp_packets_received.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = track.poll().await {
                match event {
                    TrackRemoteEvent::OnOpen(_) => {
                        track_open_count.fetch_add(1, Ordering::SeqCst);
                    }
                    TrackRemoteEvent::OnRtpPacket(_) => {
                        rtp_packets_received.fetch_add(1, Ordering::SeqCst);
                    }
                    TrackRemoteEvent::OnEnded => break,
                    _ => {}
                }
            }
        }));
    }
}

fn new_video_track(stream_id: &str, track_id: &str, ssrc: u32) -> Arc<TrackLocalStaticRTP> {
    Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        stream_id.to_owned(),
        track_id.to_owned(),
        format!("track-{track_id}"),
        RtpCodecKind::Video,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(ssrc),
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
    )))
}

#[test]
fn test_media_only_negotiation_does_not_start_sctp() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let mut offerer_media = MediaEngine::default();
    offerer_media.register_default_codecs()?;
    let answerer_media = offerer_media.clone();

    let offerer_unexpected_dcs = Arc::new(AtomicU32::new(0));
    let answerer_unexpected_dcs = Arc::new(AtomicU32::new(0));
    let answerer_track_open_count = Arc::new(AtomicU32::new(0));
    let answerer_rtp_packets = Arc::new(AtomicU32::new(0));

    let (offerer_gather_tx, mut offerer_gather_rx) = channel::<()>(1);
    let (offerer_connected_tx, mut offerer_connected_rx) = channel::<()>(1);
    let (answerer_gather_tx, mut answerer_gather_rx) = channel::<()>(1);
    let (answerer_connected_tx, mut answerer_connected_rx) = channel::<()>(1);

    let offerer = Arc::new(
        PeerConnectionBuilder::new()
            .with_media_engine(offerer_media)
            .with_handler(Arc::new(OffererHandler {
                gather_complete_tx: offerer_gather_tx,
                connected_tx: offerer_connected_tx,
                unexpected_data_channels: offerer_unexpected_dcs.clone(),
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await?,
    );

    let answerer = Arc::new(
        PeerConnectionBuilder::new()
            .with_media_engine(answerer_media)
            .with_handler(Arc::new(AnswererHandler {
                gather_complete_tx: answerer_gather_tx,
                connected_tx: answerer_connected_tx,
                unexpected_data_channels: answerer_unexpected_dcs.clone(),
                track_open_count: answerer_track_open_count.clone(),
                rtp_packets_received: answerer_rtp_packets.clone(),
                runtime: runtime.clone(),
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await?,
    );

    let video_ssrc = rand::random::<u32>();
    let video_track = new_video_track("media-only-stream", "video-track", video_ssrc);
    offerer
        .add_track(
            Arc::clone(&video_track) as Arc<dyn webrtc::media_stream::track_local::TrackLocal>
        )
        .await?;

    let offer = offerer.create_offer(None).await?;
    offerer.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offerer_gather_rx.recv()).await;
    let offer_sdp = offerer
        .local_description()
        .await
        .expect("offerer local description should be set");

    assert!(
        !offer_sdp
            .sdp
            .lines()
            .any(|line| line.starts_with("m=application ")),
        "media-only offer should not contain an application m-line:\n{}",
        offer_sdp.sdp
    );

    answerer.set_remote_description(offer_sdp).await?;
    let answer = answerer.create_answer(None).await?;
    answerer.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answerer_gather_rx.recv()).await;
    let answer_sdp = answerer
        .local_description()
        .await
        .expect("answerer local description should be set");

    assert!(
        !answer_sdp
            .sdp
            .lines()
            .any(|line| line.starts_with("m=application ")),
        "media-only answer should not contain an application m-line:\n{}",
        answer_sdp.sdp
    );

    offerer.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), offerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout waiting for offerer to connect"))?;
    timeout(Duration::from_secs(15), answerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout waiting for answerer to connect"))?;

    for seq in 0u16..20 {
        video_track
            .write_rtp(rtc::rtp::packet::Packet {
                header: rtc::rtp::header::Header {
                    version: 2,
                    payload_type: 96,
                    sequence_number: seq,
                    timestamp: seq as u32 * 3000,
                    ssrc: video_ssrc,
                    ..Default::default()
                },
                payload: bytes::Bytes::from_static(&[0xAA, 0xBB, 0xCC, 0xDD]),
            })
            .await?;
        sleep(Duration::from_millis(10)).await;
    }

    let wait_start = Instant::now();
    while answerer_rtp_packets.load(Ordering::SeqCst) < 10 {
        if wait_start.elapsed() > Duration::from_secs(10) {
            anyhow::bail!(
                "timeout waiting for RTP packets, track_open_count={}, received={}",
                answerer_track_open_count.load(Ordering::SeqCst),
                answerer_rtp_packets.load(Ordering::SeqCst)
            );
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(
        answerer_track_open_count.load(Ordering::SeqCst) > 0,
        "media-only negotiation should open a remote track"
    );

    assert_eq!(
        offerer_unexpected_dcs.load(Ordering::SeqCst),
        0,
        "media-only negotiation should not emit offerer data-channel callbacks"
    );
    assert_eq!(
        answerer_unexpected_dcs.load(Ordering::SeqCst),
        0,
        "media-only negotiation should not emit answerer data-channel callbacks"
    );

    offerer.close().await?;
    answerer.close().await?;

    Ok(())
}
