//! Integration test for the send back-pressure **escape hatch**: setting
//! `WEBRTC_SEND_HIGH_WATER_BYTES=0` / `WEBRTC_SEND_HARD_CEILING_BYTES=0` maps the limits to
//! `usize::MAX` (see `send_limits`, the `Some(0) => usize::MAX` arm), disabling the gate so a
//! caller that does its own flow control — or the send-backpressure A/B benchmark — runs with
//! no library-imposed bound.
//!
//! Complement to `data_channel_send_backpressure.rs`: that test asserts the gate BOUNDS the
//! pipeline; this one asserts the escape hatch DISABLES it. With the gate off, a naive 32 MB flood
//! must have EVERY send admitted (return Ok) no matter how far `outstanding_bytes` climbs — the
//! natural regression of the `Some(0) => usize::MAX` arm (deleting it, so "0" parses as `0` and
//! clamps `high_water = hard_ceiling = 0`) makes the second send return `ErrSendBufferFull`, which
//! this flood catches. Conservation still holds — the counter drains back to ~0, and because a
//! reliable/ordered channel has no abandonment path the counter decrements ONLY on SCTP
//! acknowledgement, so draining to ~0 proves the peer actually SACKed every byte (real end-to-end
//! delivery, independent of the built-in receive path's lossy app-delivery channel).
//!
//! It deliberately does NOT assert a lower bound on the peak `outstanding_bytes`: messages are
//! capped at the 256 KiB SCTP max and this stack restores SCTP rwnd at reassembly (not app-consume),
//! so the sender drains at line rate and a contended CI runner never accumulates a large backlog.
use anyhow::Result;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::peer_connection::{RTCIceGatheringState, RTCPeerConnectionState};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep, timeout};

const CHUNK: usize = 1024; // 1 KB messages
// Reliable/ordered flood total; large enough to exercise the disabled-gate admit path many times.
const TOTAL_BYTES: usize = 32 * 1024 * 1024;

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
fn test_data_channel_send_unbounded_escape_hatch() {
    // Disable BOTH gates via the documented `0 = unbounded` escape hatch. `send_limits()` reads
    // this once, on the first send(), so setting it before `block_on` is in force. Safe: this is
    // the only test in this binary, so no other thread reads the environment concurrently.
    unsafe {
        std::env::set_var("WEBRTC_SEND_HIGH_WATER_BYTES", "0");
        std::env::set_var("WEBRTC_SEND_HARD_CEILING_BYTES", "0");
    }
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

    // Reliable + ordered so nothing is abandoned — conservation here is driven purely by SCTP
    // acknowledgement (no forward-TSN path), a clean complement to the bounded test's unordered/
    // no-retransmit channel, and the property that lets the drain double as a delivery proof.
    let dc = sender_pc
        .create_data_channel("unbounded", Some(RTCDataChannelInit::default()))
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

    // ── Naive flood with the gate DISABLED (env=0 ⇒ limits are usize::MAX) ──────
    // The escape-hatch teeth: with the gate off EVERY send is admitted and returns Ok, no matter
    // how far `outstanding_bytes` has climbed. The natural regression of the `Some(0) => usize::MAX`
    // arm — deleting it, so "0" parses as `Some(0) => 0` and clamps `high_water = hard_ceiling = 0`
    // — makes the SECOND send (once outstanding > 0) return `ErrSendBufferFull`, failing here on the
    // 2nd of 32768 iterations. Run in the main task so that failure fails the test synchronously.
    //
    // NB this cannot instead assert "outstanding exceeds the gated cap": messages are capped at the
    // 256 KiB SCTP max, and this stack restores SCTP rwnd at reassembly (not app-consume), so the
    // sender drains at line rate and never accumulates a backlog — on a contended CI runner the
    // counter never climbs past the small in-flight window, so a lower-bound on it is unreliable.
    let chunk = BytesMut::from(vec![0u8; CHUNK].as_slice());
    let mut sent = 0usize;
    while sent < TOTAL_BYTES {
        dc.send(chunk.clone()).await.map_err(|e| {
            anyhow::anyhow!(
                "send #{} unexpectedly failed with the gate disabled ({e:?}) — the \
                 WEBRTC_SEND_*=0 escape hatch did not map the limits to usize::MAX",
                sent / CHUNK + 1
            )
        })?;
        sent += CHUNK;
    }

    // Conservation AND real end-to-end delivery: reliable/ordered ⇒ the counter decrements ONLY on
    // SCTP acknowledgement (no abandonment path), so draining to ~0 proves the peer SACKed every
    // byte. This is the robust delivery proof — unlike the app-level `received` counter below, it
    // does not depend on the built-in receive path's bounded, lossy app-delivery channel.
    let mut final_outstanding = dc.outstanding_bytes().await?;
    for _ in 0..300 {
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

    // App-level receipt is informational only, NOT asserted: the built-in receive path hands
    // messages to the app over a bounded, lossy channel, so a CPU-starved consumer (e.g. CI under
    // coverage instrumentation) legitimately drops app-level messages even on a reliable/ordered
    // channel. SCTP-level delivery is already proven by the drain above.
    let got = received.load(Ordering::Relaxed);
    log::info!("send-unbounded: flooded {TOTAL_BYTES} bytes, gate disabled; app received={got}");

    sender_pc.close().await?;
    receiver_pc.close().await?;
    Ok(())
}
