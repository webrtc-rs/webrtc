//! mDNS-enabled ICE candidate resolution example
//!
//! The peers still exchange SDP offers/answers through the example's normal
//! in-process signaling flow; mDNS here is only used for privacy-preserving
//! host candidates and for resolving remote `.local` ICE candidates.
//! Both peers use `MulticastDnsMode::QueryAndGather` so that:
//!
//! - **QueryAndGather**: Local candidates advertise a `.local` mDNS hostname
//!   instead of exposing the raw IP address (privacy-preserving).
//! - Remote `.local` candidates are resolved via multicast DNS on the local
//!   network -- no STUN server is needed for that local hostname resolution.
//!
//! ## How to run
//!
//! ```sh
//! cargo run --example mdns-local-peers
//! ```
//!
//! Both peers will run in the same process and exchange a data channel message
//! to verify end-to-end connectivity.
//!
//! ## Notes
//!
//! - mDNS requires access to the `224.0.0.251:5353` multicast group.  Some
//!   environments (CI, containers without multicast routing) may prevent the
//!   socket from joining the group.  When that happens the peer connection
//!   builder logs a warning and continues without mDNS -- `.local` candidates
//!   will not be advertised or resolved, but the connection can still succeed
//!   via other candidate types (host, srflx, relay).
//! - For true cross-host mDNS peer discovery you would run one peer on each
//!   host and exchange their SDP offers/answers via a signaling channel.

use std::sync::Arc;
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    MulticastDnsMode, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCIceGatheringState, RTCPeerConnectionState, SettingEngine,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const TEST_MESSAGE: &str = "Hello via mDNS-enabled peer connection!";

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

    // Configure mDNS: QueryAndGather means local candidates use .local
    // hostnames AND remote .local candidates are resolved via multicast DNS.
    // with_mdns_mode() sets the mode on both the async wrapper and the sans-IO core.
    let mut offerer_se = SettingEngine::default();
    offerer_se.set_multicast_dns_local_name("offerer-webrtc.local".to_string());

    // ── Offerer ────────────────────────────────────────────────────────────────
    let offerer_pc = PeerConnectionBuilder::new()
        .with_setting_engine(offerer_se)
        .with_mdns_mode(MulticastDnsMode::QueryAndGather)
        .with_handler(Arc::new(OffererHandler {
            gather_tx: offerer_gather_tx,
            connected_tx: offerer_connected_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await?;

    let offerer_dc = offerer_pc.create_data_channel("chat", None).await?;
    eprintln!("Offerer: created data channel");

    // Track when the data channel opens
    {
        let dc = offerer_dc.clone();
        let open_tx = offerer_dc_open_tx.clone();
        runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                if let DataChannelEvent::OnOpen = event {
                    eprintln!("Offerer: data channel opened");
                    open_tx.try_send(()).ok();
                }
            }
        }));
    }

    let offer = offerer_pc.create_offer(None).await?;
    offerer_pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), offerer_gather_rx.recv()).await;
    let offer_sdp = offerer_pc.local_description().await.expect("offerer SDP");
    eprintln!("Offerer: ICE gathering complete");

    // ── Answerer ───────────────────────────────────────────────────────────────
    let mut answerer_se = SettingEngine::default();
    answerer_se.set_multicast_dns_local_name("answerer-webrtc.local".to_string());

    let answerer_pc = PeerConnectionBuilder::new()
        .with_setting_engine(answerer_se)
        .with_mdns_mode(MulticastDnsMode::QueryAndGather)
        .with_handler(Arc::new(AnswererHandler {
            gather_tx: answerer_gather_tx,
            connected_tx: answerer_connected_tx,
            msg_tx: answerer_msg_tx,
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await?;

    answerer_pc.set_remote_description(offer_sdp).await?;
    let answer = answerer_pc.create_answer(None).await?;
    answerer_pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), answerer_gather_rx.recv()).await;
    let answer_sdp = answerer_pc.local_description().await.expect("answerer SDP");
    eprintln!("Answerer: ICE gathering complete");

    offerer_pc.set_remote_description(answer_sdp).await?;

    // ── Wait for connection ────────────────────────────────────────────────────
    timeout(Duration::from_secs(15), offerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for offerer to connect"))?;
    eprintln!("Offerer: connected!");

    timeout(Duration::from_secs(5), answerer_connected_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for answerer to connect"))?;
    eprintln!("Answerer: connected!");

    // ── Send message ───────────────────────────────────────────────────────────
    timeout(Duration::from_secs(10), offerer_dc_open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for data channel to open"))?;

    eprintln!("Offerer: sending '{}'", TEST_MESSAGE);
    offerer_dc.send_text(TEST_MESSAGE).await?;

    let received = timeout(Duration::from_secs(10), answerer_msg_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for message"))?
        .ok_or_else(|| anyhow::anyhow!("Channel closed"))?;

    assert_eq!(received, TEST_MESSAGE);
    eprintln!("✅ Message received: '{}'", received);

    sleep(Duration::from_millis(100)).await;
    offerer_pc.close().await?;
    answerer_pc.close().await?;

    eprintln!("✅ mDNS-local-peers example completed");
    Ok(())
}
