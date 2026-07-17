//! Integration test for the **blocking** send back-pressure path: `DataChannel::send` with a
//! configured `with_data_channel_send_buffer_limit`.
//!
//! Unlike `try_send` (which fails fast — see `data_channel_send_backpressure.rs`), a blocking
//! `send()` over a **reliable / ordered** channel with a limit set must (a) never let the
//! send pipeline grow past the limit, yet (b) still deliver every byte end-to-end, awaiting
//! capacity as the peer acknowledges rather than dropping or erroring. This is the
//! `tokio::mpsc::Sender::send`-shaped happy path.
//!
//! A naive sender floods `TOTAL_BYTES` with the blocking `send()` (no `bufferedAmount`
//! pacing of its own — the library blocks it) while a normally-draining receiver reads. The
//! test asserts:
//!
//!   1. **Bounded** — a concurrently sampled `outstanding_bytes()` never exceeds the limit
//!      plus a small slack (one in-flight message) for the whole flood. A regression that
//!      stopped blocking would let it climb toward the SCTP window and past this bound.
//!   2. **Delivered + conserved** — after the flood, `outstanding_bytes()` drains to ~0.
//!      On a reliable/ordered channel the counter decrements ONLY on SCTP acknowledgement
//!      (no abandonment path), so draining to ~0 proves the blocking sender actually pushed
//!      every byte through and the peer SACKed it — real end-to-end delivery, and no leak.
//!
//! `send()` never returns `ErrSendBufferFull` here (that is `try_send`); any error is fatal.
use anyhow::Result;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const CHUNK: usize = 4096; // 4 KB messages
// Reliable/ordered flood total; large enough to keep the blocking gate engaged for a while.
const TOTAL_BYTES: usize = 8 * 1024 * 1024;
// Small per-channel send-buffer limit, below the ~1 MiB SCTP window so the blocking gate
// engages regardless of scheduling.
const SEND_LIMIT: usize = 256 * 1024;
// A single sender admits at most one message beyond the limit before it must re-await
// capacity, so `outstanding_bytes()` can never exceed SEND_LIMIT + CHUNK; the 64 KiB slack
// only covers sampling. This bound is far below what a non-blocking pipeline would reach, so
// a regression that stopped blocking makes the bounded assertion FAIL.
const OUTSTANDING_BOUND: usize = SEND_LIMIT + 64 * 1024;

struct GatherHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for GatherHandler {
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

struct ReceiverHandler {
    gather_tx: Sender<()>,
    connected_tx: Sender<()>,
    received: Arc<AtomicUsize>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for ReceiverHandler {
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
        let received = self.received.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        received.fetch_add(msg.data.len(), Ordering::Relaxed);
                    }
                    DataChannelEvent::OnClose | DataChannelEvent::OnError => break,
                    _ => {}
                }
            }
        }));
    }
}

#[test]
fn test_data_channel_blocking_send_bounded_and_delivered() {
    block_on(run()).unwrap();
}

async fn run() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init()
        .ok();

    let runtime = default_runtime().ok_or_else(|| std::io::Error::other("no async runtime"))?;

    let (snd_gather_tx, mut snd_gather_rx) = channel::<()>(1);
    let (snd_conn_tx, mut snd_conn_rx) = channel::<()>(1);
    let (rcv_gather_tx, mut rcv_gather_rx) = channel::<()>(1);
    let (rcv_conn_tx, mut rcv_conn_rx) = channel::<()>(1);
    let received = Arc::new(AtomicUsize::new(0));

    // ── Sender (with an explicit small send-buffer limit ⇒ blocking send) ───────
    let sender_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(GatherHandler {
            gather_tx: snd_gather_tx,
            connected_tx: snd_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_data_channel_send_buffer_limit(SEND_LIMIT)
        .build()
        .await?;

    // Reliable + ordered so nothing is abandoned — conservation is driven purely by SCTP
    // acknowledgement, which lets the drain double as an end-to-end delivery proof.
    let dc = sender_pc
        .create_data_channel("blocking", Some(RTCDataChannelInit::default()))
        .await?;

    let (open_tx, mut open_rx) = channel::<()>(1);
    {
        let dc = dc.clone();
        runtime.spawn(Box::pin(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        let _ = open_tx.try_send(());
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        }));
    }

    let offer = sender_pc.create_offer(None).await?;
    sender_pc.set_local_description(offer).await?;
    let _ = timeout(Duration::from_secs(5), snd_gather_rx.recv()).await;
    let offer_sdp = sender_pc
        .local_description()
        .await
        .expect("sender local description");

    // ── Receiver ──────────────────────────────────────────────────────────────
    let receiver_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(ReceiverHandler {
            gather_tx: rcv_gather_tx,
            connected_tx: rcv_conn_tx,
            received: received.clone(),
            runtime: runtime.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    receiver_pc.set_remote_description(offer_sdp).await?;
    let answer = receiver_pc.create_answer(None).await?;
    receiver_pc.set_local_description(answer).await?;
    let _ = timeout(Duration::from_secs(5), rcv_gather_rx.recv()).await;
    let answer_sdp = receiver_pc
        .local_description()
        .await
        .expect("receiver local description");
    sender_pc.set_remote_description(answer_sdp).await?;

    timeout(Duration::from_secs(15), snd_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: sender connect"))?;
    timeout(Duration::from_secs(5), rcv_conn_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: receiver connect"))?;
    timeout(Duration::from_secs(10), open_rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timeout: data channel open"))?;

    // ── Naive flood with the BLOCKING send() (library paces it, not the app) ────
    let flood_done = Arc::new(AtomicBool::new(false));
    let flood_fatal = Arc::new(AtomicBool::new(false));
    {
        let dc = dc.clone();
        let flood_done = flood_done.clone();
        let flood_fatal = flood_fatal.clone();
        runtime.spawn(Box::pin(async move {
            let chunk = BytesMut::from(vec![0u8; CHUNK].as_slice());
            let mut sent = 0usize;
            while sent < TOTAL_BYTES {
                if let Err(e) = dc.send(chunk.clone()).await {
                    log::error!("blocking flood: unexpected send error: {e:?}");
                    flood_fatal.store(true, Ordering::Relaxed);
                    break;
                }
                sent += CHUNK;
            }
            flood_done.store(true, Ordering::Relaxed);
        }));
    }

    // Property 1 — bounded. The blocking gate must hold outstanding at/under the limit for
    // the whole flood; this upper bound cannot fail spuriously when the gate works.
    let mut max_outstanding = 0usize;
    let deadline_ticks = 600; // 600 * 100ms = 60s cap
    let mut ticks = 0;
    while !flood_done.load(Ordering::Relaxed) {
        let o = dc.outstanding_bytes().await?;
        max_outstanding = max_outstanding.max(o);
        assert!(
            o <= OUTSTANDING_BOUND,
            "outstanding_bytes {o} exceeded the send-buffer bound {OUTSTANDING_BOUND} during \
             the blocking flood — send() is not blocking on the send-buffer limit"
        );
        ticks += 1;
        assert!(
            ticks < deadline_ticks,
            "blocking flood did not complete within 60s"
        );
        sleep(Duration::from_millis(100)).await;
    }
    assert!(
        !flood_fatal.load(Ordering::Relaxed),
        "blocking flood hit an unexpected send error (see log)"
    );

    // Property 2 — delivered + conserved. Reliable/ordered ⇒ the counter drains to ~0 only
    // once the peer SACKs every byte, so this is both a leak check and an end-to-end delivery
    // proof for the blocking path.
    let mut final_outstanding = dc.outstanding_bytes().await?;
    for _ in 0..300 {
        final_outstanding = dc.outstanding_bytes().await?;
        if final_outstanding < 64 * 1024 {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(
        final_outstanding < 64 * 1024,
        "outstanding_bytes did not drain (leak / undelivered?): {final_outstanding} bytes \
         still outstanding"
    );

    let got = received.load(Ordering::Relaxed);
    log::info!("blocking-send: max_outstanding={max_outstanding} app_received={got}");

    sender_pc.close().await?;
    receiver_pc.close().await?;
    Ok(())
}
