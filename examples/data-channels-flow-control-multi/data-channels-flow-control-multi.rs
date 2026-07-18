//! Multi-pair data-channel flow-control bench for a single-`poop` N-connection run.
//!
//! Spawns `FLOW_PAIRS` in-process connection pairs, each running the unordered/no-retransmit,
//! 1 KB / 512 KB-1 MB-watermark, `bufferedAmount`-flow-controlled transfer of the stock
//! `data-channels-flow-control` example. Each pair skips `FLOW_WARMUP_MB` of ramp, then
//! measures per-interval steady-state throughput over `FLOW_STOP_MB`; once every pair has
//! finished its measured window the process prints an aggregate `FINAL` line and exits.
//! Fixed work per run ⇒ a clean `poop` command:
//!
//!   FLOW_PAIRS=10 FLOW_WARMUP_MB=64 FLOW_STOP_MB=128 FLOW_DEDICATED_REACTOR=1 \
//!     poop ./multi-master ./multi-backpressure ./pion-multi
//!
//! Uses ONLY public APIs, so the same source builds on upstream master and on the
//! send-back-pressure branch (no `outstanding_bytes()` dependency).

use bytes::BytesMut;
use futures::FutureExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
// Application message size is set at runtime via FLOW_MSG_KB (default 1 KB).

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

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
    warmup_bytes: usize,
    stop_bytes: usize,
    stop: Arc<AtomicBool>,
    mbps_tx: Sender<f64>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for ResponderHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let warmup = self.warmup_bytes;
        let stop_bytes = self.stop_bytes;
        let stop = self.stop.clone();
        let mbps_tx = self.mbps_tx.clone();
        // Must spawn: returning from on_data_channel unblocks the driver.
        self.runtime.spawn(Box::pin(async move {
            let mut got = 0usize;
            let mut measure_start: Option<Instant> = None;
            let mut measure_start_bytes = 0usize;
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(msg) => {
                        got += msg.data.len();
                        // Start the clock once past the warmup threshold.
                        if measure_start.is_none() && got >= warmup {
                            measure_start = Some(Instant::now());
                            measure_start_bytes = got;
                        }
                        if let Some(start) = measure_start
                            && got - measure_start_bytes >= stop_bytes
                        {
                            let secs = start.elapsed().as_secs_f64();
                            let measured = got - measure_start_bytes;
                            let mbps = (measured * 8) as f64 / secs / (1024.0 * 1024.0);
                            let _ = mbps_tx.try_send(mbps);
                            stop.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    DataChannelEvent::OnClose | DataChannelEvent::OnError => break,
                    _ => {}
                }
            }
        }));
    }
}

async fn build_pc(
    runtime: Arc<dyn Runtime>,
    dedicated: bool,
    handler: Arc<dyn PeerConnectionEventHandler>,
) -> anyhow::Result<Arc<dyn PeerConnection>> {
    let mut media = MediaEngine::default();
    media.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut media)?;
    let pc = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().build())
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .with_dedicated_reactor_thread(dedicated)
        .build()
        .await?;
    Ok(Arc::new(pc) as Arc<dyn PeerConnection>)
}

/// Build + connect one unordered/no-retransmit pair; spawn its flow-controlled send loop and a
/// measuring receive loop. Returns both peer connections so the caller keeps them alive.
async fn run_pair(
    runtime: Arc<dyn Runtime>,
    dedicated: bool,
    warmup_bytes: usize,
    stop_bytes: usize,
    chunk_bytes: usize,
    ordered: bool,
    mbps_tx: Sender<f64>,
) -> anyhow::Result<[Arc<dyn PeerConnection>; 2]> {
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

    // Default: unordered / no-retransmit (matches the stock data-channels-flow-control
    // example and pion's). FLOW_ORDERED=1 switches to ordered + reliable (no
    // max_retransmits), matching the batch-drain issue-101 comment's harness.
    let dc_init = if ordered {
        RTCDataChannelInit {
            ordered: true,
            ..Default::default()
        }
    } else {
        RTCDataChannelInit {
            ordered: false,
            max_retransmits: Some(0),
            ..Default::default()
        }
    };
    let dc = requester.create_data_channel("data", Some(dc_init)).await?;
    dc.set_buffered_amount_low_threshold(BUFFERED_LOW).await?;
    dc.set_buffered_amount_high_threshold(BUFFERED_HIGH).await?;

    // Flow-controlled send loop (single-task; mirrors data-channels-flow-control).
    {
        let stop = stop.clone();
        runtime.spawn(Box::pin(async move {
            let buf = BytesMut::from(vec![0u8; chunk_bytes].as_slice());
            let mut dc_open = false;
            let mut paused = false;
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
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
            warmup_bytes,
            stop_bytes,
            stop,
            mbps_tx,
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

    Ok([requester, responder])
}

fn main() -> anyhow::Result<()> {
    block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let pairs = env_usize("FLOW_PAIRS", 10);
    let warmup_bytes = env_usize("FLOW_WARMUP_MB", 64) * 1024 * 1024;
    let stop_bytes = env_usize("FLOW_STOP_MB", 128) * 1024 * 1024;
    // Application message size. Large messages (e.g. 64) fragment into many MTU-sized
    // SCTP DATA chunks -> many same-size datagrams to one peer, which the driver
    // coalesces into single UDP GSO syscalls; 1 (the default) is one datagram/message.
    let chunk_bytes = env_usize("FLOW_MSG_KB", 1) * 1024;
    // FLOW_ORDERED=1 -> ordered + reliable data channel (issue-101 batch-drain harness);
    // default is unordered / no-retransmit.
    let ordered = std::env::var("FLOW_ORDERED")
        .map(|v| v != "0")
        .unwrap_or(false);
    let dedicated = std::env::var("FLOW_DEDICATED_REACTOR")
        .map(|v| v != "0")
        .unwrap_or(true);

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;
    let (mbps_tx, mut mbps_rx) = channel::<f64>(pairs.max(1));

    let start = Instant::now();

    // Set up all pairs (sequential handshake; transfers run concurrently once open).
    let mut held: Vec<Arc<dyn PeerConnection>> = Vec::with_capacity(pairs * 2);
    for _ in 0..pairs {
        let [a, b] = run_pair(
            runtime.clone(),
            dedicated,
            warmup_bytes,
            stop_bytes,
            chunk_bytes,
            ordered,
            mbps_tx.clone(),
        )
        .await?;
        held.push(a);
        held.push(b);
    }

    // Wait for every pair to finish its measured window, summing per-pair steady-state.
    let mut agg_mbps = 0.0f64;
    for _ in 0..pairs {
        if let Some(mbps) = mbps_rx.recv().await {
            agg_mbps += mbps;
        }
    }

    let secs = start.elapsed().as_secs_f64();
    println!(
        "FINAL {agg_mbps:.3} Mbps aggregate over {pairs} pairs (dedicated={dedicated}) in {secs:.3}s",
    );

    drop(held);
    Ok(())
}
