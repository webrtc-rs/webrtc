//! Multi-pair data-channel stress bench — the reproducible A/B harness for
//! **send back-pressure** (peak RSS / throughput under N concurrent transfers).
//!
//! Creates `STRESS_PAIRS` in-process connection pairs concurrently and pushes
//! `STRESS_MB_PER_PAIR` MB over an unordered / no-retransmit data channel per pair
//! (1 KB chunks), then prints an aggregate line (throughput + VmRSS + drain) and
//! EXITS. Fixed work per run ⇒ a clean `poop` command.
//!
//! ## Environment knobs
//! * `STRESS_PAIRS` (default 10), `STRESS_MB_PER_PAIR` (default 64) — the workload.
//! * `STRESS_DEDICATED_REACTOR=1` — run each peer's driver on a dedicated reactor
//!   thread (issue #101).
//! * `STRESS_NAIVE_SENDER=1` — sender does NO app-level `bufferedAmount` flow
//!   control; it just floods the blocking `send()` in a loop. This is the realistic
//!   "app that never checks bufferedAmount" case: with a send-buffer limit configured,
//!   `send()` blocks once the channel is at the limit so peak queued memory stays
//!   bounded; with no limit (the default) it never blocks and the whole transfer piles
//!   into the send pipeline. Without this knob, the sender self-throttles on
//!   `bufferedAmount`.
//! * `STRESS_DRAIN_CHECK=1` — after the transfer, poll each channel's
//!   `outstanding_bytes()` and print it, verifying the send counter drains to ~0
//!   (conservation, including `max_retransmits: 0` abandonment).
//!
//! ## A/B'ing the send-buffer limit
//! `STRESS_SEND_BUFFER_LIMIT` sets the per-channel send-buffer limit in bytes (`0` or
//! unset = unbounded, the library default). The SAME binary serves as both arms — only
//! the limit differs. Arm A is the unbounded default; arm B opts into a limit:
//!
//!   STRESS_NAIVE_SENDER=1 STRESS_PAIRS=30 STRESS_MB_PER_PAIR=32 poop \
//!     './stress' \
//!     'env STRESS_SEND_BUFFER_LIMIT=4194304 ./stress'
//!
//! (unbounded → bounded: the bounded arm's `peak_rss` stays flat as the transfer grows,
//! whereas the unbounded arm's scales with total bytes in flight. A 16 MiB limit —
//! Chromium's cap — roughly halves peak RSS on this workload; a smaller 4 MiB limit cuts
//! it further. See the PR description for the measured numbers.)

use bytes::BytesMut;
use futures::FutureExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::peer_connection::{
    MediaEngine, RTCConfigurationBuilder, RTCIceGatheringState, Registry,
    register_default_interceptors,
};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime};

const BUFFERED_LOW: u32 = 512 * 1024; // 512 KB
const BUFFERED_HIGH: u32 = 1024 * 1024; // 1 MB
const CHUNK: usize = 1024; // 1 KB messages (max per-message reactor pressure)

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(target_os = "linux")]
fn vm_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn reactor_threads() -> usize {
    std::fs::read_dir("/proc/self/task")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            std::fs::read_to_string(e.path().join("comm"))
                .map(|c| c.trim() == "webrtc-reactor")
                .unwrap_or(false)
        })
        .count()
}

// ── Handlers ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct GatherHandler {
    gather_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for GatherHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }
}

#[derive(Clone)]
struct ResponderHandler {
    runtime: Arc<dyn Runtime>,
    gather_tx: Sender<()>,
    target: usize,
    received_total: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    done_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for ResponderHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let target = self.target;
        let received_total = self.received_total.clone();
        let stop = self.stop.clone();
        let done_tx = self.done_tx.clone();
        // Must spawn: returning from on_data_channel unblocks the driver.
        self.runtime.spawn(Box::pin(async move {
            let mut got = 0usize;
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        got += msg.data.len();
                        received_total.fetch_add(msg.data.len(), Ordering::Relaxed);
                        if got >= target {
                            stop.store(true, Ordering::Relaxed);
                            let _ = done_tx.try_send(());
                            break;
                        }
                    }
                    DataChannelEvent::OnClose | DataChannelEvent::OnError => {
                        let _ = done_tx.try_send(());
                        break;
                    }
                    _ => {}
                }
            }
        }));
    }
}

// ── Pair setup ────────────────────────────────────────────────────────────────

async fn build_pc(
    runtime: Arc<dyn Runtime>,
    dedicated: bool,
    handler: Arc<dyn PeerConnectionEventHandler>,
) -> anyhow::Result<Arc<dyn PeerConnection>> {
    let mut media = MediaEngine::default();
    media.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut media)?;
    let mut builder = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().build())
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_dedicated_reactor_thread(dedicated);
    // A/B knob for the send-buffer cap: `STRESS_SEND_BUFFER_LIMIT` (bytes; `0` = unbounded).
    // Unset ⇒ the library default (16 MiB). This is what toggles the two arms of the bench.
    if let Ok(v) = std::env::var("STRESS_SEND_BUFFER_LIMIT") {
        if let Ok(limit) = v.parse::<usize>() {
            builder = builder.with_data_channel_send_buffer_limit(limit);
        }
    }
    let pc = builder.build().await?;
    Ok(Arc::new(pc) as Arc<dyn PeerConnection>)
}

/// Build + connect one pair; spawn its flow-controlled send loop and receive-count
/// loop. Returns both peer connections so the caller keeps them alive.
async fn run_pair(
    runtime: Arc<dyn Runtime>,
    dedicated: bool,
    target: usize,
    received_total: Arc<AtomicUsize>,
    done_tx: Sender<()>,
) -> anyhow::Result<([Arc<dyn PeerConnection>; 2], Arc<dyn DataChannel>)> {
    let stop = Arc::new(AtomicBool::new(false));

    // Requester
    let (req_gather_tx, mut req_gather_rx) = channel::<()>(1);
    let requester = build_pc(
        runtime.clone(),
        dedicated,
        Arc::new(GatherHandler {
            gather_tx: req_gather_tx,
        }),
    )
    .await?;

    let dc = requester
        .create_data_channel(
            "data",
            Some(RTCDataChannelInit {
                ordered: false,
                max_retransmits: Some(0),
                ..Default::default()
            }),
        )
        .await?;
    dc.set_buffered_amount_low_threshold(BUFFERED_LOW).await?;
    dc.set_buffered_amount_high_threshold(BUFFERED_HIGH).await?;
    let dc_handle = dc.clone();

    // Send loop. Two modes:
    //  * default: single-task app-level flow control on `bufferedAmount`
    //    (mirrors data-channels-flow-control).
    //  * STRESS_NAIVE_SENDER: NO manual flow control — just flood the blocking `send()`.
    //    This is the realistic "app that never checks bufferedAmount" case: with a
    //    send-buffer limit configured, `send()` blocks once the channel is at the limit so
    //    peak queued memory stays bounded; with no limit (the default) it never blocks and
    //    the whole transfer piles into the send pipeline (peak RSS ∝ target). Both modes
    //    send the same total work, so peak RSS isolates the queue bound.
    let naive = std::env::var("STRESS_NAIVE_SENDER").is_ok();
    {
        let stop = stop.clone();
        runtime.spawn(Box::pin(async move {
            let buf = BytesMut::from(vec![0u8; CHUNK].as_slice());
            let mut dc_open = false;
            let mut paused = false;
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if naive {
                    // Drive to OnOpen first, then flood the blocking `send()` without
                    // consulting bufferedAmount until the responder has received `target`
                    // and sets `stop`. Keeping the send loop running (rather than stopping
                    // at exactly `target` bytes) guarantees termination even if the
                    // unreliable channel drops a chunk. With a send-buffer limit, `send()`
                    // blocks at the limit so peak queued memory stays bounded; without one,
                    // the sender races far ahead of the receiver and the pipeline balloons.
                    if !dc_open {
                        match dc.poll().await {
                            Some(DataChannelEvent::OnOpen) => dc_open = true,
                            Some(DataChannelEvent::OnClose) | None => break,
                            _ => {}
                        }
                        continue;
                    }
                    // Blocking send: the library paces us when a limit is set. A real error
                    // means the channel closed — stop.
                    if dc.send(buf.clone()).await.is_err() {
                        break;
                    }
                    continue;
                }
                if dc_open && !paused {
                    futures::select! {
                        maybe_event = dc.poll().fuse() => match maybe_event {
                            Some(DataChannelEvent::OnBufferedAmountHigh) => paused = true,
                            Some(DataChannelEvent::OnBufferedAmountLow) => paused = false,
                            Some(DataChannelEvent::OnClose) | None => break,
                            _ => {}
                        },
                        result = dc.send(buf.clone()).fuse() => { let _ = result; }
                    }
                } else {
                    match dc.poll().await {
                        Some(DataChannelEvent::OnOpen) => dc_open = true,
                        Some(DataChannelEvent::OnBufferedAmountHigh) => paused = true,
                        Some(DataChannelEvent::OnBufferedAmountLow) => paused = false,
                        Some(DataChannelEvent::OnClose) | None => break,
                        _ => {}
                    }
                }
            }
        }));
    }

    let offer = requester.create_offer(None).await?;
    requester.set_local_description(offer).await?;
    req_gather_rx.recv().await;
    let offer_sdp = requester
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("requester has no local description"))?;

    // Responder
    let (resp_gather_tx, mut resp_gather_rx) = channel::<()>(1);
    let responder = build_pc(
        runtime.clone(),
        dedicated,
        Arc::new(ResponderHandler {
            runtime: runtime.clone(),
            gather_tx: resp_gather_tx,
            target,
            received_total,
            stop,
            done_tx,
        }),
    )
    .await?;

    responder.set_remote_description(offer_sdp).await?;
    let answer = responder.create_answer(None).await?;
    responder.set_local_description(answer).await?;
    resp_gather_rx.recv().await;
    let answer_sdp = responder
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("responder has no local description"))?;
    requester.set_remote_description(answer_sdp).await?;

    Ok(([requester, responder], dc_handle))
}

fn main() -> anyhow::Result<()> {
    block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let pairs = env_usize("STRESS_PAIRS", 10);
    let mb_per_pair = env_usize("STRESS_MB_PER_PAIR", 64);
    let dedicated = std::env::var("STRESS_DEDICATED_REACTOR")
        .map(|v| v != "0")
        .unwrap_or(true);
    let target = mb_per_pair * 1024 * 1024;

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;
    let received_total = Arc::new(AtomicUsize::new(0));
    let (done_tx, mut done_rx) = channel::<()>(pairs.max(1));

    let start = Instant::now();

    // Set up all pairs (sequential handshake; transfers run concurrently once open).
    let mut held: Vec<Arc<dyn PeerConnection>> = Vec::with_capacity(pairs * 2);
    let mut senders: Vec<Arc<dyn DataChannel>> = Vec::with_capacity(pairs);
    for _ in 0..pairs {
        let ([a, b], dc) = run_pair(
            runtime.clone(),
            dedicated,
            target,
            received_total.clone(),
            done_tx.clone(),
        )
        .await?;
        held.push(a);
        held.push(b);
        senders.push(dc);
    }

    // Wait for every pair to receive its full target.
    for _ in 0..pairs {
        done_rx.recv().await;
    }

    // Conservation check: after the transfer stops, the per-channel send counter
    // must drain toward 0 as SCTP acks/abandons the last in-flight bytes. A leak
    // (e.g. abandoned no-retransmit bytes not decremented) would plateau instead.
    if std::env::var("STRESS_DRAIN_CHECK").is_ok() {
        for _ in 0..50 {
            let mut sum = 0usize;
            for dc in &senders {
                sum += dc.outstanding_bytes().await.unwrap_or(0);
            }
            eprintln!(
                "DRAIN outstanding_total_MB={:.2}",
                sum as f64 / (1024.0 * 1024.0)
            );
            if sum < 2 * 1024 * 1024 {
                break;
            }
            webrtc::runtime::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    let secs = start.elapsed().as_secs_f64();
    let total = received_total.load(Ordering::Relaxed);
    let agg_mbps = (total * 8) as f64 / secs / (1024.0 * 1024.0);
    let mbytes = total as f64 / (1024.0 * 1024.0);

    #[cfg(target_os = "linux")]
    eprintln!(
        "STRESS pairs={pairs} mb_per_pair={mb_per_pair} dedicated={dedicated} \
         total_MB={mbytes:.1} secs={secs:.3} agg_Mbps={agg_mbps:.1} \
         reactor_threads={} VmRSS_MB={:.1}",
        reactor_threads(),
        vm_rss_kb() as f64 / 1024.0
    );
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "STRESS pairs={pairs} mb_per_pair={mb_per_pair} dedicated={dedicated} \
         total_MB={mbytes:.1} secs={secs:.3} agg_Mbps={agg_mbps:.1}"
    );

    // Exit without close(): peak RSS/threads reflect the full concurrent footprint,
    // and we avoid confounding the transfer measurement with teardown.
    drop(held);
    Ok(())
}
