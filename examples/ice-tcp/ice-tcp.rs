//! ICE over TCP (RFC 6544) example
//!
//! Demonstrates two in-process WebRTC peers that pass TCP addresses via
//! `PeerConnectionBuilder::with_tcp_addrs()`.
//!
//! **Current limitation:** The async driver layer does not yet bind or drive TCP
//! sockets (the `_tcp_addrs` parameter in `PeerConnectionImpl::new` is a
//! placeholder).  Until TCP transport is wired through the driver, both peers
//! also bind a UDP socket so that ICE connectivity checks can succeed.
//!
//! Once the driver gains TCP support, the UDP fallback can be removed and this
//! example will demonstrate pure TCP ICE (RFC 6544).
//!
//! ## How to run
//!
//! ```sh
//! cargo run --example ice-tcp
//! ```

use std::sync::Arc;
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const TEST_MESSAGE: &str = "Hello over TCP ICE!";

// ── Offerer handler ────────────────────────────────────────────────────────────

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
        eprintln!("Offerer connection state: {}", state);
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
}

// ── Answerer handler ───────────────────────────────────────────────────────────

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
        eprintln!("Answerer connection state: {}", state);
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let label = dc.label().await.unwrap_or_default();
        eprintln!("Answerer: received data channel '{}'", label);
        let msg_tx = self.msg_tx.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => eprintln!("Answerer: data channel opened"),
                    DataChannelEvent::OnMessage(msg) => {
                        let text = String::from_utf8(msg.data.to_vec()).unwrap_or_default();
                        eprintln!("Answerer received: '{}'", text);
                        msg_tx.try_send(text).ok();
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }
}

// ── Main ───────────────────────────────────────────────────────────────────────

fn main() {
    block_on(run()).unwrap();
}

async fn run() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let (offerer_gather_tx, mut offerer_gather_rx) = channel::<()>(1);
    let (offerer_connected_tx, mut offerer_connected_rx) = channel::<()>(1);
    let (offerer_dc_open_tx, mut offerer_dc_open_rx) = channel::<()>(1);
    let (answerer_gather_tx, mut answerer_gather_rx) = channel::<()>(1);
    let (answerer_connected_tx, mut answerer_connected_rx) = channel::<()>(1);
    let (answerer_msg_tx, mut answerer_msg_rx) = channel::<String>(8);

    // ── Answerer ────────────────────────────────────────────────────────────────
    // Pass TCP addresses via with_tcp_addrs() — these will be used once the
    // driver gains TCP transport support.  A UDP socket is also bound as a
    // fallback so the example can run today.
    let answerer_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(AnswererHandler {
            gather_tx: answerer_gather_tx,
            connected_tx: answerer_connected_tx,
            msg_tx: answerer_msg_tx,
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_tcp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;
    eprintln!("Answerer: peer connection created (UDP + TCP addrs configured)");

    // ── Offerer ────────────────────────────────────────────────────────────────
    // Same as the answerer: TCP addrs are passed but only UDP is active today.
    let offerer_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(OffererHandler {
            gather_tx: offerer_gather_tx,
            connected_tx: offerer_connected_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_tcp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let offerer_dc = offerer_pc.create_data_channel("chat", None).await?;
    eprintln!("Offerer: created data channel");

    {
        let dc = offerer_dc.clone();
        let open_tx = offerer_dc_open_tx;
        runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                if let DataChannelEvent::OnOpen = event {
                    eprintln!("Offerer: data channel opened");
                    open_tx.try_send(()).ok();
                }
            }
        }));
    }

    // ── Signaling (in-process) ─────────────────────────────────────────────────
    let offer = offerer_pc.create_offer(None).await?;
    offerer_pc.set_local_description(offer).await?;
    timeout(Duration::from_secs(5), offerer_gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer ICE gathering"))?
        .ok_or_else(|| anyhow::anyhow!("Offerer ICE gathering channel closed"))?;
    let offer_sdp = offerer_pc.local_description().await.expect("offerer SDP");
    eprintln!("Offerer: ICE gathering complete");

    answerer_pc.set_remote_description(offer_sdp).await?;
    let answer = answerer_pc.create_answer(None).await?;
    answerer_pc.set_local_description(answer).await?;
    timeout(Duration::from_secs(5), answerer_gather_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for answerer ICE gathering"))?
        .ok_or_else(|| anyhow::anyhow!("Answerer ICE gathering channel closed"))?;
    let answer_sdp = answerer_pc.local_description().await.expect("answerer SDP");
    eprintln!("Answerer: ICE gathering complete");

    offerer_pc.set_remote_description(answer_sdp).await?;

    // ── Wait for connection ────────────────────────────────────────────────────
    timeout(Duration::from_secs(15), offerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer to connect"))?
        .ok_or_else(|| anyhow::anyhow!("Offerer connection channel closed"))?;
    eprintln!("Offerer: connected!");

    timeout(Duration::from_secs(5), answerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for answerer to connect"))?
        .ok_or_else(|| anyhow::anyhow!("Answerer connection channel closed"))?;
    eprintln!("Answerer: connected!");

    // ── Send message ───────────────────────────────────────────────────────────
    timeout(Duration::from_secs(10), offerer_dc_open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for data channel to open"))?
        .ok_or_else(|| anyhow::anyhow!("Data channel open notification channel closed"))?;

    eprintln!("Offerer: sending '{}'", TEST_MESSAGE);
    offerer_dc.send_text(TEST_MESSAGE).await?;

    let received = timeout(Duration::from_secs(10), answerer_msg_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for message"))?
        .ok_or_else(|| anyhow::anyhow!("Message channel closed"))?;

    assert_eq!(received, TEST_MESSAGE);
    eprintln!("✅ Message received: '{}'", received);

    sleep(Duration::from_millis(100)).await;
    offerer_pc.close().await?;
    answerer_pc.close().await?;

    eprintln!("✅ ice-tcp example completed");
    Ok(())
}
