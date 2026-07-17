//! Integration test for data-channel send back-pressure via the **non-blocking**
//! `DataChannel::try_send` / `try_send_text` (the `outstanding_bytes` accounting + the
//! fail-fast send-buffer cap).
//!
//! A "naive" sender floods a fixed amount over an **unordered / no-retransmit**
//! (`max_retransmits: 0`) channel — no `bufferedAmount` flow control — while a
//! normally-draining receiver reads. The connection is built with a small explicit
//! send-buffer limit (`with_data_channel_send_buffer_limit`) so the gate engages quickly
//! and deterministically. `try_send`/`try_send_text` are the fail-fast APIs: over the
//! limit they return `ErrSendBufferFull` immediately rather than blocking (that is
//! `send`/`send_text`, covered by `data_channel_send_blocking.rs`). The test asserts three
//! properties:
//!
//!   1. **Bounded** — `outstanding_bytes()` never exceeds the configured limit (plus a
//!      small slack) while the app floods: the gate rejects with `ErrSendBufferFull`
//!      rather than letting the send pipeline grow without limit. This is an *upper*
//!      bound, so it can never fail spuriously when the gate works (the counter is
//!      hard-capped by construction); it only fails if the gate is removed and the
//!      pipeline grows past the bound — which is exactly the regression it guards.
//!   2. **Reject engaged** — at least one `try_send()` returned `ErrSendBufferFull`. With a
//!      tiny limit vs a large flood this is deterministic, and it is the strongest teeth:
//!      deleting the cap makes `try_send()` never reject, so this fails regardless of how
//!      fast the peer drains (whereas Property 1 only bites once outstanding actually climbs).
//!   3. **Conservation** — after the transfer, `outstanding_bytes()` drains well below the
//!      limit. The counter is decremented both on SCTP acknowledgement *and* on
//!      `max_retransmits: 0` abandonment (forward-TSN), so no path can leak it.
//!
//! App-level receipt is logged but NOT asserted (the unordered/lossy receive path drops
//! under a starved consumer — end-to-end delivery is proven by the reliable/ordered sibling
//! `data_channel_send_unbounded.rs`).
//!
//! `try_send()`/`try_send_text()` never park: a full buffer is a synchronous
//! `ErrSendBufferFull`, which the naive flood handles by yielding and retrying. See
//! `examples/data-channels-flow-control` for the idiomatic `OnBufferedAmountLow`-paced
//! sender that never trips the cap.
use anyhow::Result;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::error::Error;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{
    Runtime, Sender, block_on, channel, default_runtime, sleep, timeout, yield_now,
};

const CHUNK: usize = 1024; // 1 KB messages
// 16 MB naive flood — far larger than the limit below, so an *unbounded* (gate-disabled)
// send path would grow the pipeline well past the bound, giving Property 1 real teeth.
const TOTAL_BYTES: usize = 16 * 1024 * 1024;
// Small explicit per-channel send-buffer limit, forced via the builder. Deliberately tiny
// (64 KiB) so a 1 KB-chunk flood fills it within a single pre-yield send burst — the gate
// engages, and therefore `send()` returns `ErrSendBufferFull` at least once, on every
// runtime regardless of scheduling. That makes the reject assertion below deterministic.
const SEND_LIMIT: usize = 64 * 1024;
// The gate admits only while `outstanding + len <= SEND_LIMIT` (or onto an empty buffer),
// so with 1 KB chunks `outstanding_bytes()` can never exceed SEND_LIMIT once engaged; the
// 64 KiB slack only covers sampling. This bound is BELOW the peak a disabled gate reaches,
// so a regression that removes the cap makes this assertion FAIL.
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

    // ── Sender (with an explicit small send-buffer limit) ───────────────────────
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
    // `try_send`/`try_send_text` return `ErrSendBufferFull` (non-blocking) instead of
    // parking, so the naive sender yields and retries — never advancing `sent` on a
    // rejection. Any other error is fatal.
    let flood_done = Arc::new(AtomicBool::new(false));
    let flood_fatal = Arc::new(AtomicBool::new(false));
    let rejections = Arc::new(AtomicUsize::new(0));
    {
        let dc = dc.clone();
        let flood_done = flood_done.clone();
        let flood_fatal = flood_fatal.clone();
        let rejections = rejections.clone();
        runtime.spawn(Box::pin(async move {
            let buf = BytesMut::from(vec![0u8; CHUNK].as_slice());
            let text = "x".repeat(CHUNK);
            let mut sent = 0usize;
            let mut use_text = false;
            while sent < TOTAL_BYTES {
                // Alternate binary try_send() and try_send_text() so the cap is exercised on
                // both send paths (they share the same admit/reject logic).
                let res = if use_text {
                    dc.try_send_text(&text).await
                } else {
                    dc.try_send(buf.clone()).await
                };
                match res {
                    Ok(()) => {
                        sent += CHUNK;
                        use_text = !use_text;
                    }
                    Err(Error::ErrSendBufferFull) => {
                        rejections.fetch_add(1, Ordering::Relaxed);
                        // Non-blocking gate: back off and let the driver drain acked bytes.
                        yield_now().await;
                    }
                    Err(e) => {
                        log::error!("flood: unexpected send error: {e:?}");
                        flood_fatal.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
            flood_done.store(true, Ordering::Relaxed);
        }));
    }

    // Sample outstanding_bytes() concurrently: the cap must hold it at/under the limit for
    // the whole flood (Property 1 — bounded). This upper bound cannot fail spuriously when
    // the gate works (the counter is hard-capped), and fails if the gate is removed.
    let mut max_outstanding = 0usize;
    let deadline_ticks = 600; // 600 * 100ms = 60s cap
    let mut ticks = 0;
    while !flood_done.load(Ordering::Relaxed) {
        let o = dc.outstanding_bytes().await?;
        max_outstanding = max_outstanding.max(o);
        assert!(
            o <= OUTSTANDING_BOUND,
            "outstanding_bytes {o} exceeded the send-buffer bound {OUTSTANDING_BOUND} \
             during flood — the non-blocking try_send cap is not enforcing the bound"
        );
        ticks += 1;
        assert!(ticks < deadline_ticks, "flood did not complete within 60s");
        sleep(Duration::from_millis(100)).await;
    }
    assert!(
        !flood_fatal.load(Ordering::Relaxed),
        "flood hit a non-ErrSendBufferFull send error (see log)"
    );

    // Property 2 — the reject path actually engaged. A tiny 64 KiB limit against a 16 MB
    // flood guarantees at least one `ErrSendBufferFull`, so this is deterministic — and it
    // is the strongest teeth: deleting the cap makes `try_send()` never reject, so
    // `rejections` is 0 and this FAILS regardless of how fast the peer drains (unlike the
    // upper bound above, which only bites once outstanding actually climbs). It also proves
    // `try_send()` wasn't a silent no-op (a no-op never fills the buffer, so it never rejects).
    let n_rejections = rejections.load(Ordering::Relaxed);
    assert!(
        n_rejections > 0,
        "try_send() never returned ErrSendBufferFull over a {TOTAL_BYTES}-byte flood at a \
         {SEND_LIMIT}-byte limit — the non-blocking try_send cap did not engage"
    );

    // Property 3 — conservation: after the flood, the counter must drain well below the
    // limit as SCTP acks/abandons the last in-flight bytes. A decrement bug (e.g. abandoned
    // no-retransmit bytes never released) would plateau at the ~limit steady state instead.
    let mut final_outstanding = dc.outstanding_bytes().await?;
    for _ in 0..100 {
        final_outstanding = dc.outstanding_bytes().await?;
        if final_outstanding < SEND_LIMIT / 4 {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(
        final_outstanding < SEND_LIMIT / 4,
        "outstanding_bytes did not drain (leak?): {final_outstanding} bytes still outstanding"
    );

    // App-level receipt is informational only, NOT asserted: this is an unordered /
    // no-retransmit channel whose built-in receive path hands messages to the app over a
    // bounded, lossy channel, so a starved consumer (e.g. CI under coverage instrumentation)
    // legitimately drops app-level messages. End-to-end delivery is proven by the reliable/
    // ordered sibling test (data_channel_send_unbounded.rs) instead.
    let got = received.load(Ordering::Relaxed);
    log::info!(
        "send-backpressure: max_outstanding={max_outstanding} rejections={n_rejections} received={got}"
    );

    sender_pc.close().await?;
    receiver_pc.close().await?;
    Ok(())
}
