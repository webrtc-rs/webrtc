/// Integration test for the opt-in dedicated per-connection reactor thread (issue #101).
///
/// Both peers run their driver on a dedicated OS thread
/// (`with_dedicated_reactor_thread(true)`). This exercises the reactor path
/// end-to-end: sockets are bound up front but wrapped and driven on the reactor
/// thread, ICE/DTLS/SCTP establish, a data channel opens, a message is echoed,
/// and both connections close cleanly (which must stop the reactor threads).
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const TEST_MESSAGE: &str = "Hello over a dedicated reactor!";
const ECHO_MESSAGE: &str = "Echo over a dedicated reactor!";

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
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
}

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
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let answer_msg_tx = self.answer_msg_tx.clone();
        // NOTE: with a dedicated reactor thread this callback runs on that thread;
        // spawning the poll loop keeps it off the driver's critical path.
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        let data = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        answer_msg_tx.try_send(data).ok();
                        let _ = dc.send_text(ECHO_MESSAGE).await;
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }
}

#[test]
fn test_dedicated_reactor_data_channel() {
    block_on(run_test(false)).unwrap();
}

/// Same, but each peer also binds a TCP listener — exercises `wrap_tcp_listener`
/// running on the dedicated reactor thread (the socket<->reactor binding path).
#[test]
fn test_dedicated_reactor_data_channel_with_tcp() {
    block_on(run_test(true)).unwrap();
}

async fn run_test(with_tcp: bool) -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let (offerer_gather_tx, mut offerer_gather_rx) = channel::<()>(1);
    let (offerer_connected_tx, mut offerer_connected_rx) = channel::<()>(1);
    let (offerer_dc_open_tx, mut offerer_dc_open_rx) = channel::<()>(8);
    let (offerer_msg_tx, mut offerer_msg_rx) = channel::<String>(256);
    let (answerer_gather_tx, mut answerer_gather_rx) = channel::<()>(1);
    let (answerer_connected_tx, mut answerer_connected_rx) = channel::<()>(1);
    let (answerer_msg_tx, mut answerer_msg_rx) = channel::<String>(256);

    // ── Offerer on a dedicated reactor thread ──────────────────────────────────
    let mut offerer_builder = PeerConnectionBuilder::new()
        .with_handler(Arc::new(OffererHandler {
            gather_complete_tx: offerer_gather_tx,
            connected_tx: offerer_connected_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_dedicated_reactor_thread(true);
    if with_tcp {
        offerer_builder = offerer_builder.with_tcp_addrs(vec!["127.0.0.1:0".to_string()]);
    }
    let offerer_pc = offerer_builder.build().await?;

    let offerer_dc = offerer_pc.create_data_channel("test-channel", None).await?;
    {
        let dc = offerer_dc.clone();
        let dc_open_tx = offerer_dc_open_tx.clone();
        let msg_tx = offerer_msg_tx.clone();
        runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        dc_open_tx.try_send(()).ok();
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        msg_tx
                            .try_send(String::from_utf8(msg.data.to_vec()).unwrap_or_default())
                            .ok();
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }

    let offer = offerer_pc.create_offer(None).await?;
    offerer_pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offerer_gather_rx.recv()).await;
    let offer_sdp = offerer_pc
        .local_description()
        .await
        .expect("offerer local description should be set");

    // ── Answerer on a dedicated reactor thread ─────────────────────────────────
    let mut answerer_builder = PeerConnectionBuilder::new()
        .with_handler(Arc::new(AnswererHandler {
            gather_complete_tx: answerer_gather_tx,
            connected_tx: answerer_connected_tx,
            answer_msg_tx: answerer_msg_tx,
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_dedicated_reactor_thread(true);
    if with_tcp {
        answerer_builder = answerer_builder.with_tcp_addrs(vec!["127.0.0.1:0".to_string()]);
    }
    let answerer_pc = answerer_builder.build().await?;

    answerer_pc.set_remote_description(offer_sdp).await?;
    let answer = answerer_pc.create_answer(None).await?;
    answerer_pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answerer_gather_rx.recv()).await;
    let answer_sdp = answerer_pc
        .local_description()
        .await
        .expect("answerer local description should be set");

    offerer_pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), offerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer to connect"))?;
    timeout(Duration::from_secs(5), answerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for answerer to connect"))?;

    timeout(Duration::from_secs(10), offerer_dc_open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer data channel to open"))?;

    offerer_dc.send_text(TEST_MESSAGE).await?;

    let answer_msg = timeout(Duration::from_secs(10), answerer_msg_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for answerer to receive message"))?
        .ok_or_else(|| anyhow::anyhow!("Answerer message channel closed"))?;
    let offer_echo = timeout(Duration::from_secs(10), offerer_msg_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer to receive echo"))?
        .ok_or_else(|| anyhow::anyhow!("Offerer message channel closed"))?;

    assert_eq!(
        answer_msg, TEST_MESSAGE,
        "answerer should receive the message"
    );
    assert_eq!(offer_echo, ECHO_MESSAGE, "offerer should receive the echo");

    // Closing must stop the dedicated reactor threads cleanly.
    sleep(Duration::from_millis(100)).await;
    offerer_pc.close().await?;
    answerer_pc.close().await?;

    Ok(())
}
