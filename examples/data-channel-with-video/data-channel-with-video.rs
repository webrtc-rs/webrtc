//! DataChannel + Video transceiver on the same PeerConnection
//!
//! Demonstrates that a single [`RTCPeerConnection`] can simultaneously host:
//!
//! - An **RTP video transceiver** (`m=video` in SDP)
//! - A **data channel** (`m=application` / SCTP in SDP)
//!
//! ## Key requirement
//!
//! You **must** call [`MediaEngine::register_default_codecs`] (or register at
//! least one video codec manually) before creating an offer.  Without a codec
//! registration the SDP generator has no payload types to advertise and emits a
//! rejected `m=video 0 …` line, which can make it look as if mixing a data
//! channel with a video transceiver is broken — it is not.
//!
//! ## How to run
//!
//! ```sh
//! cargo run --example data-channel-with-video
//! ```
//!
//! Both peers run in the same process. The offerer adds a `Recvonly` video
//! transceiver and a data channel; the answerer sets the remote description
//! (which implicitly creates matching transceivers) and creates an answer. After
//! ICE+DTLS negotiate, the offerer sends a text message over the data channel
//! and the answerer prints it. No actual video RTP is sent.

use std::sync::Arc;
use std::time::Duration;

use rtc::rtp_transceiver::RTCRtpTransceiverDirection;
use rtc::rtp_transceiver::RTCRtpTransceiverInit;
use rtc::rtp_transceiver::rtp_sender::RtpCodecKind;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const TEST_MESSAGE: &str = "Hello from data channel (alongside video)!";

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

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Build a MediaEngine with all default codecs registered.
///
/// This is the critical step that ensures video m-lines in the SDP have valid
/// payload types.  Omitting it causes `m=video 0 …` (rejected) to appear in
/// the offer, which has nothing to do with mixing data channels and video.
fn make_media_engine() -> MediaEngine {
    let mut me = MediaEngine::default();
    me.register_default_codecs()
        .expect("register_default_codecs failed");
    me
}

fn recvonly_init() -> Option<RTCRtpTransceiverInit> {
    Some(RTCRtpTransceiverInit {
        direction: RTCRtpTransceiverDirection::Recvonly,
        send_encodings: vec![],
        streams: vec![],
    })
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

    // ── Offerer: video transceiver + data channel ──────────────────────────────
    let offerer_pc = PeerConnectionBuilder::new()
        .with_media_engine(make_media_engine()) // <-- required for valid m=video
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
    eprintln!("Offerer: added video transceiver (recvonly)");

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

    let offer = offerer_pc.create_offer(None).await?;
    eprintln!(
        "Offerer: SDP offer contains m=video: {}",
        offer.sdp.contains("m=video")
    );
    eprintln!(
        "Offerer: SDP offer contains m=application: {}",
        offer.sdp.contains("m=application")
    );

    offerer_pc.set_local_description(offer).await?;
    match timeout(Duration::from_secs(5), offerer_gather_rx.recv()).await {
        Ok(Some(_)) => eprintln!("Offerer: ICE gathering complete"),
        Ok(None) => return Err(anyhow::anyhow!("Offerer ICE gathering channel closed")),
        Err(_) => {
            return Err(anyhow::anyhow!(
                "Timeout: offerer ICE gathering did not complete within 5s"
            ));
        }
    }
    let offer_sdp = offerer_pc.local_description().await.expect("offerer SDP");

    // ── Answerer: mirror the offerer's configuration ───────────────────────────
    let answerer_pc = PeerConnectionBuilder::new()
        .with_media_engine(make_media_engine()) // <-- required on answerer too
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
    eprintln!(
        "Answerer: SDP answer contains m=video: {}",
        answer.sdp.contains("m=video")
    );
    eprintln!(
        "Answerer: SDP answer contains m=application: {}",
        answer.sdp.contains("m=application")
    );

    answerer_pc.set_local_description(answer).await?;
    match timeout(Duration::from_secs(5), answerer_gather_rx.recv()).await {
        Ok(Some(_)) => eprintln!("Answerer: ICE gathering complete"),
        Ok(None) => return Err(anyhow::anyhow!("Answerer ICE gathering channel closed")),
        Err(_) => {
            return Err(anyhow::anyhow!(
                "Timeout: answerer ICE gathering did not complete within 5s"
            ));
        }
    }
    let answer_sdp = answerer_pc.local_description().await.expect("answerer SDP");

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

    // ── Send message over the data channel ─────────────────────────────────────
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

    eprintln!("✅ data-channel-with-video example completed");
    Ok(())
}
