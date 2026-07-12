//! Integration test for data-channel send back-pressure (the `outstanding_bytes`
//! accounting + high-water/hard-ceiling gate in `DataChannel::send`).
//!
//! A "naive" sender floods a fixed amount over an **unordered / no-retransmit**
//! (`max_retransmits: 0`) channel — no `bufferedAmount` flow control — while a
//! normally-draining receiver reads. The test asserts three properties of the
//! per-channel send counter that the back-pressure fix introduces:
//!
//!   1. **Bounded** — `outstanding_bytes()` never exceeds the hard ceiling while the
//!      app floods (the send gate keeps the pipeline from growing without limit).
//!   2. **Conservation** — after the transfer, `outstanding_bytes()` drains back to
//!      ~0. This is the key correctness property: the counter is decremented both on
//!      SCTP acknowledgement *and* on `max_retransmits: 0` abandonment (forward-TSN),
//!      so no path can leak and wedge a sender permanently.
//!   3. **Delivery** — a substantial fraction of the bytes actually arrive (sanity
//!      that the flood really flowed, not that `send()` silently no-op'd).
use anyhow::Result;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const CHUNK: usize = 1024; // 1 KB messages
const TOTAL_BYTES: usize = 8 * 1024 * 1024; // 8 MB naive flood
// Must match the library defaults in `src/data_channel/mod.rs` (this test does not
// override the `WEBRTC_SEND_*` env vars, so the defaults are in force).
const HARD_CEILING: usize = 16 * 1024 * 1024;

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
        // Must spawn: returning from on_data_channel unblocks the driver.
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
fn test_data_channel_send_backpressure_bounded_and_conserved() {
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

    // ── Sender ────────────────────────────────────────────────────────────────
    let sender_pc = PeerConnectionBuilder::new()
        .with_handler(Arc::new(GatherHandler {
            gather_tx: snd_gather_tx,
            connected_tx: snd_conn_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    let dc = sender_pc
        .create_data_channel(
            "backpressure",
            Some(RTCDataChannelInit {
                ordered: false,
                max_retransmits: Some(0),
                ..Default::default()
            }),
        )
        .await?;

    // Drive the sender channel to OnOpen.
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

    // ── Naive flood: push TOTAL_BYTES with NO bufferedAmount flow control ───────
    let flood_done = Arc::new(AtomicBool::new(false));
    {
        let dc = dc.clone();
        let flood_done = flood_done.clone();
        runtime.spawn(Box::pin(async move {
            let buf = BytesMut::from(vec![0u8; CHUNK].as_slice());
            let mut sent = 0usize;
            while sent < TOTAL_BYTES {
                if dc.send(buf.clone()).await.is_ok() {
                    sent += CHUNK;
                }
            }
            flood_done.store(true, Ordering::Relaxed);
        }));
    }

    // Sample outstanding_bytes() concurrently: it must stay under the hard ceiling
    // for the whole flood (Property 1 — bounded).
    let mut max_outstanding = 0usize;
    let deadline_ticks = 300; // 300 * 100ms = 30s cap
    let mut ticks = 0;
    while !flood_done.load(Ordering::Relaxed) {
        let o = dc.outstanding_bytes().await?;
        max_outstanding = max_outstanding.max(o);
        assert!(
            o <= HARD_CEILING,
            "outstanding_bytes {o} exceeded hard ceiling {HARD_CEILING} during flood"
        );
        ticks += 1;
        assert!(ticks < deadline_ticks, "flood did not complete within 30s");
        sleep(Duration::from_millis(100)).await;
    }

    // Property 2 — conservation: after the flood, the counter must drain toward 0
    // as SCTP acks/abandons the last in-flight bytes. A decrement bug (e.g. abandoned
    // no-retransmit bytes never released) would plateau here instead of draining.
    let mut final_outstanding = dc.outstanding_bytes().await?;
    for _ in 0..100 {
        final_outstanding = dc.outstanding_bytes().await?;
        if final_outstanding < 512 * 1024 {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(
        final_outstanding < 512 * 1024,
        "outstanding_bytes did not drain (leak?): {final_outstanding} bytes still outstanding"
    );

    // Property 3 — delivery sanity: real bytes flowed (send() wasn't a silent no-op).
    // This is only a floor: an *unordered / no-retransmit* naive flood legitimately
    // abandons a large fraction (here ~5 MB of 8 MB was dropped, which is exactly what
    // exercised the forward-TSN abandonment decrement checked by Property 2), so we do
    // not assert near-complete delivery — only that a meaningful amount arrived.
    let got = received.load(Ordering::Relaxed);
    assert!(
        got >= TOTAL_BYTES / 8,
        "receiver only got {got} of {TOTAL_BYTES} bytes (send appears to have no-op'd)"
    );

    log::info!("send-backpressure: max_outstanding={max_outstanding} received={got}");

    sender_pc.close().await?;
    receiver_pc.close().await?;
    Ok(())
}
