/// Integration test for offer/answer between two async WebRTC peers (webrtc-to-webrtc)
///
/// This test verifies that two async WebRTC peers can establish a connection, create data
/// channels, and exchange messages using only the high-level async PeerConnection API —
/// no sansio/RTC primitives required.
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const TEST_MESSAGE: &str = "Hello from offerer!";
const ECHO_MESSAGE: &str = "Echo from answerer!";

// ── Offerer handler ────────────────────────────────────────────────────────────

struct OffererHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for OffererHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        log::info!("Offerer connection state: {}", state);
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
}

// ── Answerer handler ───────────────────────────────────────────────────────────

struct AnswererHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
    answer_msg_tx: Sender<String>,
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
        log::info!("Answerer connection state: {}", state);
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let label = dc.label().await.unwrap_or_default();
        log::info!("Answerer received data channel: {}", label);
        let answer_msg_tx = self.answer_msg_tx.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        log::info!("Answerer data channel opened");
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        let data = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        log::info!("Answerer received: '{}'", data);
                        answer_msg_tx.try_send(data).ok();
                        // Echo back
                        log::info!("Answerer echoing: '{}'", ECHO_MESSAGE);
                        if let Err(e) = dc.send_text(ECHO_MESSAGE).await {
                            log::error!("Answerer failed to echo: {}", e);
                        }
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }
}

// ── Test entry point ───────────────────────────────────────────────────────────

/// Test data channel communication between two async WebRTC peers
#[test]
fn test_offer_answer_webrtc_to_webrtc() {
    block_on(run_test()).unwrap();
}

async fn run_test() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    log::info!("Starting offer-answer test: WebRTC offerer -> WebRTC answerer");

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    // ── Channels ───────────────────────────────────────────────────────────────
    let (offerer_gather_tx, mut offerer_gather_rx) = channel::<()>(1);
    let (offerer_connected_tx, mut offerer_connected_rx) = channel::<()>(1);
    let (offerer_dc_open_tx, mut offerer_dc_open_rx) = channel::<()>(8);
    let (offerer_msg_tx, mut offerer_msg_rx) = channel::<String>(256);
    let (answerer_gather_tx, mut answerer_gather_rx) = channel::<()>(1);
    let (answerer_connected_tx, mut answerer_connected_rx) = channel::<()>(1);
    let (answerer_msg_tx, mut answerer_msg_rx) = channel::<String>(256);

    // ── Build offerer peer connection ──────────────────────────────────────────
    let offerer_handler = Arc::new(OffererHandler {
        gather_complete_tx: offerer_gather_tx,
        connected_tx: offerer_connected_tx,
    });

    let offerer_pc = PeerConnectionBuilder::new()
        .with_handler(offerer_handler)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;
    log::info!("Created offerer peer connection");

    // Create data channel on offerer side and spawn poll task
    let dc_label = "test-channel";
    let offerer_dc = offerer_pc.create_data_channel(dc_label, None).await?;
    log::info!("Offerer created data channel: {}", dc_label);

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
                        let data = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        log::info!("Offerer received: '{}'", data);
                        msg_tx.try_send(data).ok();
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }

    // Create offer and wait for ICE gathering
    let offer = offerer_pc.create_offer(None).await?;
    offerer_pc.set_local_description(offer).await?;
    log::info!("Offerer set local description");

    let _ = timeout(Duration::from_secs(5), offerer_gather_rx.recv()).await;
    let offer_sdp = offerer_pc
        .local_description()
        .await
        .expect("offerer local description should be set");
    log::info!("Offerer ICE gathering complete");

    // ── Build answerer peer connection ─────────────────────────────────────────
    let answerer_handler = Arc::new(AnswererHandler {
        gather_complete_tx: answerer_gather_tx,
        connected_tx: answerer_connected_tx,
        answer_msg_tx: answerer_msg_tx,
        runtime: runtime.clone(),
    });

    let answerer_pc = PeerConnectionBuilder::new()
        .with_handler(answerer_handler)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;
    log::info!("Created answerer peer connection");

    // Answerer sets remote description (offerer's offer) and creates answer
    answerer_pc.set_remote_description(offer_sdp).await?;
    log::info!("Answerer set remote description");

    let answer = answerer_pc.create_answer(None).await?;
    answerer_pc.set_local_description(answer).await?;
    log::info!("Answerer set local description");

    let _ = timeout(Duration::from_secs(5), answerer_gather_rx.recv()).await;
    let answer_sdp = answerer_pc
        .local_description()
        .await
        .expect("answerer local description should be set");
    log::info!("Answerer ICE gathering complete");

    // Offerer sets remote description (answerer's answer)
    offerer_pc.set_remote_description(answer_sdp).await?;
    log::info!("Offerer set remote description");

    // ── Wait for both peers to connect ─────────────────────────────────────────
    timeout(Duration::from_secs(15), offerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer to connect"))?;
    log::info!("Offerer connected!");

    timeout(Duration::from_secs(5), answerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for answerer to connect"))?;
    log::info!("Answerer connected!");

    // ── Wait for offerer DC to open, then send test message ───────────────────
    timeout(Duration::from_secs(10), offerer_dc_open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer data channel to open"))?;

    log::info!("Offerer sending: '{}'", TEST_MESSAGE);
    offerer_dc.send_text(TEST_MESSAGE).await?;

    // ── Wait for answerer to receive and echo ─────────────────────────────────
    let answer_msg = timeout(Duration::from_secs(10), answerer_msg_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for answerer to receive message"))?
        .ok_or_else(|| anyhow::anyhow!("Answerer message channel closed"))?;
    log::info!("Answerer received: '{}'", answer_msg);

    // ── Wait for offerer to receive echo ─────────────────────────────────────
    let offer_echo = timeout(Duration::from_secs(10), offerer_msg_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer to receive echo"))?
        .ok_or_else(|| anyhow::anyhow!("Offerer message channel closed"))?;
    log::info!("Offerer received echo: '{}'", offer_echo);

    // ── Assertions ────────────────────────────────────────────────────────────
    assert_eq!(
        answer_msg, TEST_MESSAGE,
        "Answerer should receive TEST_MESSAGE"
    );
    assert_eq!(
        offer_echo, ECHO_MESSAGE,
        "Offerer should receive ECHO_MESSAGE"
    );

    log::info!("✅ Offer-answer webrtc-to-webrtc test completed successfully!");

    // Give background tasks a moment to settle before closing
    sleep(Duration::from_millis(100)).await;

    offerer_pc.close().await?;
    answerer_pc.close().await?;

    Ok(())
}
