use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_VP8, MediaEngine};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::{fs::OpenOptions, io::Write as IoWrite, str::FromStr, time::Duration};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep};

const TRACK_SWAP_INTERVAL: Duration = Duration::from_secs(5);

// ── Shared output state (mirrors sansio's output_timestamp / output_sequence) ─

struct OutputState {
    timestamp: u32,
    sequence: u16,
}

// ── Shared state ──────────────────────────────────────────────────────────────

struct Shared {
    curr_track_idx: AtomicUsize,
    track_count: AtomicUsize,
    output_ssrc: u32,
    output_track: Arc<TrackLocalStaticRTP>,
    /// Serializes writes to output_track so timestamp/sequence stay monotonic.
    output_state: Mutex<OutputState>,
    /// (ssrc, track_remote) per track_num — for PLI on track switch.
    track_info: Mutex<HashMap<usize, (u32, Arc<dyn TrackRemote>)>>,
}

// ── Event handler ─────────────────────────────────────────────────────────────

struct SwapTracksHandler {
    shared: Arc<Shared>,
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
    done_tx: Sender<()>,
    runtime: Arc<dyn Runtime>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for SwapTracksHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer Connection State has changed: {state}");
        match state {
            RTCPeerConnectionState::Connected => {
                let _ = self.connected_tx.try_send(());
            }
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                let _ = self.done_tx.try_send(());
            }
            _ => {}
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let shared = self.shared.clone();
        let runtime = self.runtime.clone();

        self.runtime.spawn(Box::pin(async move {
            // track_num is assigned from receiver_id (= transceiver/m-line index)
            // so it matches the browser's stream order regardless of RTP arrival order.
            let mut track_num: usize = 0;
            // Per-track timestamp delta tracking — mirrors sansio's
            // receiver_last_timestamp map, updated for every packet regardless
            // of whether this track is active.
            let mut last_ts: Option<u32> = None;
            let mut is_current = false;

            while let Some(evt) = track.poll().await {
                match evt {
                    TrackRemoteEvent::OnOpen(init) => {
                        // Use the transceiver index (= SDP m-line order) as
                        // track_num so browser and server agree on numbering.
                        track_num = usize::from(init.receiver_id);
                        shared
                            .track_count
                            .fetch_max(track_num + 1, Ordering::SeqCst);
                        println!("Track {} has started, ssrc={}", track_num + 1, init.ssrc);
                        {
                            let mut info = shared.track_info.lock().unwrap();
                            info.insert(track_num, (init.ssrc, track.clone()));
                        }
                        // Periodic PLI every 3 s — keeps all tracks' keyframes
                        // fresh even when inactive, matching sansio behaviour.
                        let pli_track = track.clone();
                        let media_ssrc = init.ssrc;
                        runtime.spawn(Box::pin(async move {
                            loop {
                                sleep(Duration::from_secs(3)).await;
                                if pli_track
                                    .write_rtcp(vec![Box::new(PictureLossIndication {
                                        sender_ssrc: 0,
                                        media_ssrc,
                                    })])
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }));
                    }

                    TrackRemoteEvent::OnRtpPacket(mut pkt) => {
                        let orig_ts = pkt.header.timestamp;
                        // Compute delta BEFORE the active-track check, exactly
                        // as sansio does with receiver_last_timestamp — so the
                        // delta is accurate when this track becomes active.
                        let ts_delta = last_ts.map(|lt| orig_ts.wrapping_sub(lt)).unwrap_or(0);
                        last_ts = Some(orig_ts);

                        // Only forward if this is the currently active track.
                        let curr_idx = shared.curr_track_idx.load(Ordering::SeqCst);
                        if curr_idx != track_num {
                            is_current = false;
                            continue;
                        }

                        // PLI on track switch to get a fresh keyframe quickly.
                        if !is_current {
                            is_current = true;
                            let ssrc_and_track = {
                                let info = shared.track_info.lock().unwrap();
                                info.get(&track_num).map(|(s, t)| (*s, t.clone()))
                            };
                            if let Some((ssrc, track_ref)) = ssrc_and_track {
                                let _ = track_ref
                                    .write_rtcp(vec![Box::new(PictureLossIndication {
                                        sender_ssrc: 0,
                                        media_ssrc: ssrc,
                                    })])
                                    .await;
                            }
                        }

                        // Apply monotonic output timestamp and sequence — same
                        // as sansio's output_timestamp / output_sequence.
                        {
                            let mut out = shared.output_state.lock().unwrap();
                            out.timestamp = out.timestamp.wrapping_add(ts_delta);
                            out.sequence = out.sequence.wrapping_add(1);
                            pkt.header.timestamp = out.timestamp;
                            pkt.header.sequence_number = out.sequence;
                        }
                        pkt.header.ssrc = shared.output_ssrc;

                        if let Err(err) = shared.output_track.write_rtp(pkt).await {
                            println!("write_rtp error: {err}");
                            break;
                        }
                    }

                    TrackRemoteEvent::OnEnded => {
                        println!("Track {} ended", track_num + 1);
                        break;
                    }
                    _ => {}
                }
            }
        }));
    }
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "swap-tracks")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "An example of swapping tracks using the async WebRTC API.")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    block_on(async_main())
}

async fn async_main() -> Result<()> {
    let cli = Cli::parse();
    let log_level = log::LevelFilter::from_str(&cli.log_level)?;

    if cli.debug {
        env_logger::Builder::new()
            .target(if !cli.output_log_file.is_empty() {
                Target::Pipe(Box::new(
                    OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(&cli.output_log_file)?,
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

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    // ── Media engine: VP8 only ────────────────────────────────────────────────

    let mut media_engine = MediaEngine::default();
    let video_codec = RTCRtpCodec {
        mime_type: MIME_TYPE_VP8.to_owned(),
        clock_rate: 90000,
        ..Default::default()
    };
    media_engine.register_codec(
        RTCRtpCodecParameters {
            rtp_codec: video_codec.clone(),
            payload_type: 96,
            ..Default::default()
        },
        RtpCodecKind::Video,
    )?;

    let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;

    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    // ── Output track ──────────────────────────────────────────────────────────

    let output_ssrc: u32 = rand::random();
    let output_track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        "webrtc-rs".to_string(),
        "video".to_string(),
        "video".to_string(),
        RtpCodecKind::Video,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(output_ssrc),
                ..Default::default()
            },
            codec: video_codec.clone(),
            ..Default::default()
        }],
    )));

    // ── Shared state ──────────────────────────────────────────────────────────

    let shared = Arc::new(Shared {
        curr_track_idx: AtomicUsize::new(0),
        track_count: AtomicUsize::new(0),
        output_ssrc,
        output_track: output_track.clone(),
        output_state: Mutex::new(OutputState {
            timestamp: 0,
            sequence: 0,
        }),
        track_info: Mutex::new(HashMap::new()),
    });

    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>(1);
    let (connected_tx, mut connected_rx) = channel::<()>(1);
    let (done_tx, mut done_rx) = channel::<()>(1);

    // ── Peer connection ───────────────────────────────────────────────────────

    let handler = Arc::new(SwapTracksHandler {
        shared: shared.clone(),
        gather_complete_tx,
        connected_tx,
        done_tx: done_tx.clone(),
        runtime: runtime.clone(),
    });

    let pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_handler(handler as Arc<dyn PeerConnectionEventHandler>)
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
            .build()
            .await?,
    );

    pc.add_track(Arc::clone(&output_track) as Arc<dyn TrackLocal>)
        .await?;

    // ── Signaling ─────────────────────────────────────────────────────────────

    println!("Paste offer from browser and press Enter:");
    let line = if cli.input_sdp_file.is_empty() {
        signal::must_read_stdin()?
    } else {
        std::fs::read_to_string(&cli.input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    println!("Offer received");

    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    pc.set_local_description(answer).await?;

    let _ = webrtc::runtime::timeout(Duration::from_secs(5), gather_complete_rx.recv()).await;

    if let Some(local_desc) = pc.local_description().await {
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = signal::encode(&json_str);
        println!("{b64}");
    } else {
        anyhow::bail!("generate local_description failed!");
    }

    // ── Track swap timer ──────────────────────────────────────────────────────
    // Waits for connection, then rotates the active track every 5 seconds,
    // mirroring sansio's timer-based swap + PLI-on-switch logic.

    let shared_swap = shared.clone();
    runtime.spawn(Box::pin(async move {
        let _ = connected_rx.recv().await;
        println!("Connection established, will swap tracks every {TRACK_SWAP_INTERVAL:?}");

        loop {
            sleep(TRACK_SWAP_INTERVAL).await;

            let count = shared_swap.track_count.load(Ordering::SeqCst);
            if count == 0 {
                continue;
            }

            let curr = shared_swap.curr_track_idx.load(Ordering::SeqCst);
            let next = (curr + 1) % count;
            shared_swap.curr_track_idx.store(next, Ordering::SeqCst);
            println!("Switched from track {} to track {}", curr + 1, next + 1);

            // PLI for the newly active track — matches sansio's on-switch PLI.
            let ssrc_and_track = {
                let info = shared_swap.track_info.lock().unwrap();
                info.get(&next).map(|(s, t)| (*s, t.clone()))
            };
            if let Some((ssrc, track_ref)) = ssrc_and_track {
                let _ = track_ref
                    .write_rtcp(vec![Box::new(PictureLossIndication {
                        sender_ssrc: 0,
                        media_ssrc: ssrc,
                    })])
                    .await;
            }
        }
    }));

    // ── Wait for ctrl-c or peer connection failure ────────────────────────────

    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    println!("Press Ctrl-C to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("Peer connection closed or failed, shutting down.");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!("Received ctrl-c, shutting down.");
        }
    }

    pc.close().await?;
    Ok(())
}
