//! Data Channels Flow Control Example
//!
//! Demonstrates flow control via `OnBufferedAmountHigh` / `OnBufferedAmountLow` events.
//! Two in-process peer connections are created: a requester that sends data as fast as
//! possible and a responder that measures throughput.
//!
//! When the send buffer exceeds MAX_BUFFERED_AMOUNT the sender pauses; it resumes once
//! the buffer drains below BUFFERED_AMOUNT_LOW_THRESHOLD.

use bytes::BytesMut;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::data_channel::RTCDataChannelInit;
use rtc::interceptor::Registry;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::MediaEngine;
use rtc::peer_connection::state::{RTCIceGatheringState, RTCPeerConnectionState};
use std::fs::OpenOptions;
use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime};

const BUFFERED_AMOUNT_LOW_THRESHOLD: u32 = 5120 * 1024; // 5120 KB
const BUFFERED_AMOUNT_HIGH_THRESHOLD: u32 = 102400 * 1024; // 100 MB

// ── Requester handler ────────────────────────────────────────────────────────

#[derive(Clone)]
struct RequesterHandler {
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for RequesterHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("[requester] Connection state: {state}");
        if state == RTCPeerConnectionState::Failed {
            let _ = self.done_tx.try_send(());
        }
    }
}

// ── Responder handler ────────────────────────────────────────────────────────

#[derive(Clone)]
struct ResponderHandler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for ResponderHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("[responder] Connection state: {state}");
        if state == RTCPeerConnectionState::Failed {
            let _ = self.done_tx.try_send(());
        }
    }

    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        let done_tx = self.done_tx.clone();
        // Must spawn: returning from on_data_channel unblocks the driver;
        // awaiting poll() here would stall it.
        self.runtime.spawn(Box::pin(async move {
            let mut total_bytes: usize = 0;
            let mut last_bytes: usize = 0;
            let mut period_start = Instant::now();

            while let Some(event) = data_channel.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        println!("[responder] Data channel open — measuring throughput...");
                        period_start = Instant::now();
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        total_bytes += msg.data.len();
                        // Print once per second, triggered by the first message after each period
                        let now = Instant::now();
                        if now.duration_since(period_start) >= Duration::from_secs(1) {
                            let elapsed = now.duration_since(period_start);
                            let bps =
                                ((total_bytes - last_bytes) * 8) as f64 / elapsed.as_secs_f64();
                            println!("Throughput is about {:.03} Mbps", bps / (1024.0 * 1024.0));
                            last_bytes = total_bytes;
                            period_start = now;
                        }
                    }
                    DataChannelEvent::OnClose => {
                        let _ = done_tx.try_send(());
                        break;
                    }
                    _ => {}
                }
            }
        }));
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "data-channels")]
#[command(author = "Rusty Rain <y@liu.mx>")]
#[command(version = "0.0.0")]
#[command(about = "An example of Data-Channels", long_about = None)]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let output_log_file = cli.output_log_file;
    let log_level = log::LevelFilter::from_str(&cli.log_level)?;
    if cli.debug {
        env_logger::Builder::new()
            .target(if !output_log_file.is_empty() {
                Target::Pipe(Box::new(
                    OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(output_log_file)?,
                ))
            } else {
                Target::Stdout
            })
            .format(|buf, record| {
                writeln!(
                    buf,
                    "{}:{} [{}] {} - {}",
                    record.file().unwrap_or("unknown"),
                    record.line().unwrap_or(0),
                    record.level(),
                    chrono::Local::now().format("%H:%M:%S.%6f"),
                    record.args()
                )
            })
            .filter(None, log_level)
            .init();
    }

    block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let (done_tx, mut done_rx) = channel::<()>();
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>();
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    // ── Build requester peer connection ──────────────────────────────────────
    let (req_gather_tx, mut req_gather_rx) = channel::<()>();
    let mut req_media = MediaEngine::default();
    req_media.register_default_codecs()?;
    let req_registry = register_default_interceptors(Registry::new(), &mut req_media)?;

    let mut requester = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().build())
        .with_media_engine(req_media)
        .with_interceptor_registry(req_registry)
        .with_handler(Arc::new(RequesterHandler {
            gather_complete_tx: req_gather_tx,
            done_tx: done_tx.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    // Create data channel (unordered, no retransmits — maximises raw throughput)
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

    // Configure flow-control thresholds
    dc.set_buffered_amount_low_threshold(BUFFERED_AMOUNT_LOW_THRESHOLD)
        .await?;
    dc.set_buffered_amount_high_threshold(BUFFERED_AMOUNT_HIGH_THRESHOLD)
        .await?;

    // Single-task flow-control loop.
    //
    // Both event-polling and sending live in the same spawned task, so flow-control
    // state is a plain local `bool` — no cross-task AtomicBool or Notify needed.
    //
    //  • not open yet  → block on poll(), waiting for OnOpen
    //  • open, running → select! { event from poll() | send one 32 KB chunk }
    //  • open, paused  → block on poll(), waiting for OnBufferedAmountLow
    {
        runtime.spawn(Box::pin(async move {
            let buf = BytesMut::from(vec![0u8; 1024].as_slice());
            let mut dc_open = false;
            let mut paused = false;

            loop {
                if dc_open && !paused {
                    futures::select! {
                        maybe_event = dc.poll().fuse() => {
                            match maybe_event {
                                Some(DataChannelEvent::OnBufferedAmountHigh) => {
                                    println!("[requester] Data channel 1 OnBufferedAmountHigh...");
                                    paused = true;
                                }
                                Some(DataChannelEvent::OnBufferedAmountLow) => {
                                    if paused {
                                        println!("[requester] Data channel 1 OnBufferedAmountLow...");
                                    }
                                    paused = false;
                                }
                                Some(DataChannelEvent::OnClose) | None => break,
                                _ => {}
                            }
                        }
                        result = dc.send(buf.clone()).fuse() => {
                            if result.is_err() { break; }
                        }
                    }
                } else {
                    match dc.poll().await {
                        Some(DataChannelEvent::OnOpen) => {
                            println!("[requester] Data channel open — sending at full speed...");
                            dc_open = true;
                        }
                        Some(DataChannelEvent::OnBufferedAmountHigh) => {
                            println!("[requester] Data channel 2 OnBufferedAmountHigh...");
                            paused = true;
                        }
                        Some(DataChannelEvent::OnBufferedAmountLow) => {
                            if paused {
                                println!("[requester] Data channel 2 OnBufferedAmountLow...");
                            }
                            paused = false;
                        }
                        Some(DataChannelEvent::OnClose) | None => break,
                        _ => {}
                    }
                }
            }
        }));
    }

    // Create offer and wait for ICE gathering to complete
    let offer = requester.create_offer(None).await?;
    requester.set_local_description(offer).await?;
    req_gather_rx.recv().await;
    let offer_sdp = requester
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("requester has no local description"))?;

    // ── Build responder peer connection ──────────────────────────────────────
    let (resp_gather_tx, mut resp_gather_rx) = channel::<()>();
    let mut resp_media = MediaEngine::default();
    resp_media.register_default_codecs()?;
    let resp_registry = register_default_interceptors(Registry::new(), &mut resp_media)?;

    let mut responder = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().build())
        .with_media_engine(resp_media)
        .with_interceptor_registry(resp_registry)
        .with_handler(Arc::new(ResponderHandler {
            runtime: runtime.clone(),
            gather_complete_tx: resp_gather_tx,
            done_tx: done_tx.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["127.0.0.1:0".to_string()])
        .build()
        .await?;

    // In-process signaling: set the offer, create an answer, wait for ICE
    responder.set_remote_description(offer_sdp).await?;
    let answer = responder.create_answer(None).await?;
    responder.set_local_description(answer).await?;
    resp_gather_rx.recv().await;
    let answer_sdp = responder
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("responder has no local description"))?;

    // Complete the offer/answer exchange
    requester.set_remote_description(answer_sdp).await?;

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("Peer connection failed or data channel closed.");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!();
        }
    }

    requester.close().await?;
    responder.close().await?;

    Ok(())
}
