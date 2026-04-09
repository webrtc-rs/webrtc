/// Integration test: DataChannel + Video transceiver SDP coexistence (#784)
///
/// Validates that a PeerConnection can negotiate SDP containing both a video
/// m-line and a DataChannel (SCTP) m-line, and that the DataChannel works.
///
/// The test confirms:
///   1. `create_offer()` succeeds and produces SDP containing both m-lines
///   2. The answerer can parse the offer and generate a valid answer with both
///      m-lines (video is accepted, not rejected with port 0)
///   3. ICE + DTLS + SCTP establish successfully (data channel opens)
///   4. A message can be sent/received over the data channel
///
/// Default codecs are registered so video m-lines have real codec payloads.
/// No actual video RTP/frames are sent -- the test only validates SDP
/// coexistence and DataChannel behavior.
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use rtc::rtp_transceiver::RTCRtpTransceiverDirection;
use rtc::rtp_transceiver::RTCRtpTransceiverInit;
use rtc::rtp_transceiver::rtp_sender::RtpCodecKind;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::*;
use webrtc::peer_connection::{MediaEngine, RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const TEST_MESSAGE: &str = "DC over video+DC peer connection";

// ── Handlers ──────────────────────────────────────────────────────────────────

struct OffererHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for OffererHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }
    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
}

struct AnswererHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    msg_tx: Sender<String>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for AnswererHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }
    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let label = dc.label().await.unwrap_or_default();
        log::info!("Answerer received data channel: {}", label);
        let msg_tx = self.msg_tx.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => log::info!("Answerer data channel opened"),
                    DataChannelEvent::OnMessage(msg) => {
                        let text = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        log::info!("Answerer received: '{}'", text);
                        msg_tx.try_send(text).ok();
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_media_engine() -> MediaEngine {
    let mut me = MediaEngine::default();
    me.register_default_codecs()
        .expect("register_default_codecs failed");
    me
}

// ── Test entry point ──────────────────────────────────────────────────────────

/// Verify that a video transceiver and a data channel coexist on the same PeerConnection.
#[test]
fn test_datachannel_and_video_transceiver() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    log::info!("Starting DataChannel + Video transceiver test (#784)");

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let (offerer_gather_tx, mut offerer_gather_rx) = channel::<()>(1);
    let (offerer_connected_tx, mut offerer_connected_rx) = channel::<()>(1);
    let (offerer_dc_open_tx, mut offerer_dc_open_rx) = channel::<()>(1);
    // Receiver kept alive so offerer_msg_tx.send() doesn't fail; one-way test only.
    let (offerer_msg_tx, _offerer_msg_rx) = channel::<String>(8);
    let (answerer_gather_tx, mut answerer_gather_rx) = channel::<()>(1);
    let (answerer_connected_tx, mut answerer_connected_rx) = channel::<()>(1);
    let (answerer_msg_tx, mut answerer_msg_rx) = channel::<String>(8);

    let recvonly_init = || {
        Some(RTCRtpTransceiverInit {
            direction: RTCRtpTransceiverDirection::Recvonly,
            send_encodings: vec![],
            streams: vec![],
        })
    };

    // ── Build offerer ──────────────────────────────────────────────────────────
    let offerer_pc = PeerConnectionBuilder::new()
        .with_media_engine(make_media_engine())
        .with_handler(Arc::new(OffererHandler {
            gather_tx: offerer_gather_tx,
            connected_tx: offerer_connected_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    offerer_pc
        .add_transceiver_from_kind(RtpCodecKind::Video, recvonly_init())
        .await?;
    log::info!("Offerer: added video transceiver (recvonly)");

    let offerer_dc = offerer_pc.create_data_channel("test", None).await?;
    log::info!("Offerer: created data channel");

    {
        let dc = offerer_dc.clone();
        let dc_open_tx = offerer_dc_open_tx.clone();
        let msg_tx = offerer_msg_tx.clone();
        runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        log::info!("Offerer data channel opened");
                        dc_open_tx.try_send(()).ok();
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        let text = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        log::info!("Offerer received: '{}'", text);
                        msg_tx.try_send(text).ok();
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }

    let offer = offerer_pc.create_offer(None).await?;
    log::debug!("Offerer created offer:\n{}", offer.sdp);

    // Both m-lines must be present in the offer with valid ports
    assert!(
        offer.sdp.contains("m=video") && !offer.sdp.contains("m=video 0 "),
        "Offer must contain active m=video (not rejected), got:\n{}",
        offer.sdp
    );
    assert!(
        offer.sdp.contains("m=application"),
        "Offer must contain m=application (SCTP), got:\n{}",
        offer.sdp
    );
    log::info!("✅ Offer SDP contains both m=video (active) and m=application");

    offerer_pc.set_local_description(offer).await?;
    timeout(Duration::from_secs(5), offerer_gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout: offerer ICE gathering did not complete"))?
        .ok_or_else(|| anyhow::anyhow!("Offerer ICE gathering channel closed before completion"))?;
    let offer_sdp = offerer_pc
        .local_description()
        .await
        .expect("offerer local description must be set");

    // ── Build answerer (also has video + DC) ───────────────────────────────────
    let answerer_pc = PeerConnectionBuilder::new()
        .with_media_engine(make_media_engine())
        .with_handler(Arc::new(AnswererHandler {
            gather_tx: answerer_gather_tx,
            connected_tx: answerer_connected_tx,
            msg_tx: answerer_msg_tx,
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    answerer_pc.set_remote_description(offer_sdp).await?;
    let answer = answerer_pc.create_answer(None).await?;
    log::debug!("Answerer created answer:\n{}", answer.sdp);

    let has_video_mline = answer.sdp.lines().any(|line| line.starts_with("m=video "));
    let video_mline_rejected = answer
        .sdp
        .lines()
        .any(|line| line.starts_with("m=video 0 "));

    assert!(
        has_video_mline,
        "Answer must contain m=video, got:\n{}",
        answer.sdp
    );
    assert!(
        !video_mline_rejected,
        "Answer must not reject the video m-line (found `m=video 0 ...`), got:\n{}",
        answer.sdp
    );
    assert!(
        answer.sdp.contains("m=application"),
        "Answer must contain m=application, got:\n{}",
        answer.sdp
    );
    log::info!("✅ Answer SDP contains both m=video and m=application, and video is accepted");

    answerer_pc.set_local_description(answer).await?;
    timeout(Duration::from_secs(5), answerer_gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout: answerer ICE gathering did not complete"))?
        .ok_or_else(|| {
            anyhow::anyhow!("Answerer ICE gathering channel closed before completion")
        })?;
    let answer_sdp = answerer_pc
        .local_description()
        .await
        .expect("answerer local description must be set");

    offerer_pc.set_remote_description(answer_sdp).await?;

    // ── Wait for both to connect ───────────────────────────────────────────────
    timeout(Duration::from_secs(15), offerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout: offerer did not connect"))?;
    log::info!("Offerer connected");

    timeout(Duration::from_secs(5), answerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout: answerer did not connect"))?;
    log::info!("Answerer connected");

    // ── Send message over data channel ─────────────────────────────────────────
    timeout(Duration::from_secs(10), offerer_dc_open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout: offerer data channel did not open"))?;

    log::info!("Offerer sending: '{}'", TEST_MESSAGE);
    offerer_dc.send_text(TEST_MESSAGE).await?;

    let received = timeout(Duration::from_secs(10), answerer_msg_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout: answerer did not receive message"))?
        .ok_or_else(|| anyhow::anyhow!("Answerer message channel closed"))?;

    assert_eq!(
        received, TEST_MESSAGE,
        "Answerer must receive the test message"
    );
    log::info!("✅ Data channel message received over video+DC peer connection");

    // Offerer_msg_rx is intentionally unused — we only test one-way delivery here
    drop(_offerer_msg_rx);

    sleep(Duration::from_millis(100)).await;
    offerer_pc.close().await?;
    answerer_pc.close().await?;

    log::info!("✅ test_datachannel_and_video_transceiver passed");
    Ok(())
}
