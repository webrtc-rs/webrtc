use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::rtp_transceiver::rtp_sender::RtpCodecKind;
use rtc::statistics::StatsSelector;
use rtc::statistics::stats::RTCStatsType;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{fs, io::Write, str::FromStr};
use webrtc::error::Result;
use webrtc::media_stream::track_remote::TrackRemote;
use webrtc::peer_connection::{
    MediaEngine, RTCConfigurationBuilder, RTCIceGatheringState, RTCIceServer,
    RTCPeerConnectionState, RTCSessionDescription, Registry, register_default_interceptors,
};
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::{Mutex, Sender, block_on, channel, default_runtime, interval};

const STATS_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Parser)]
#[command(name = "stats")]
#[command(author = "Rusty Rain <y@liu.mx>")]
#[command(version = "0.0.0")]
#[command(about = "Demonstrates how to use the webrtc-stats implementation.")]
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

#[derive(Clone)]
struct TestHandler {
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    track_id_to_codec: Arc<Mutex<HashMap<String, String>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for TestHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        println!("ICE gathering state: {:?}", state);
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer Connection State has changed: {state}");
        if state == RTCPeerConnectionState::Failed {
            let _ = self.done_tx.try_send(());
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let kind = track.kind().await;
        let codec_name = if kind == RtpCodecKind::Audio {
            "audio/opus"
        } else {
            "video/vp8"
        };
        let track_id = track.track_id().await;
        let mut map = self.track_id_to_codec.lock().await;
        map.insert(track_id.clone(), codec_name.to_string());
        println!(
            "New incoming track: {} with codec: {}",
            track_id, codec_name
        );
    }
}

fn main() -> Result<()> {
    block_on(async_main())
}

async fn async_main() -> Result<()> {
    let cli = Cli::parse();
    let input_sdp_file = cli.input_sdp_file;
    let output_log_file = cli.output_log_file;
    let log_level = log::LevelFilter::from_str(&cli.log_level).unwrap();
    if cli.debug {
        env_logger::Builder::new()
            .target(if !output_log_file.is_empty() {
                Target::Pipe(Box::new(
                    OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(output_log_file)
                        .unwrap(),
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

    let (done_tx, mut done_rx) = channel::<()>(1);
    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>(1);

    let runtime = default_runtime().unwrap();
    let track_id_to_codec = Arc::new(Mutex::new(HashMap::new()));

    let handler = Arc::new(TestHandler {
        gather_complete_tx,
        done_tx,
        track_id_to_codec: track_id_to_codec.clone(),
    });

    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    let registry = Registry::new();
    let registry = register_default_interceptors(registry, &mut media_engine)?;

    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["0.0.0.0:0".to_owned()])
        .build()
        .await?;

    let peer_connection = Arc::new(peer_connection);

    // Wait for the offer to be pasted
    print!("Paste offer from browser and press Enter: ");
    std::io::stdout().flush().unwrap();

    let line = if input_sdp_file.is_empty() {
        signal::must_read_stdin().unwrap()
    } else {
        fs::read_to_string(&input_sdp_file).unwrap()
    };
    let desc_data = signal::decode(line.as_str()).unwrap();
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data).unwrap();
    println!("Offer received: {offer}");

    peer_connection.set_remote_description(offer).await?;

    let answer = peer_connection.create_answer(None).await?;
    peer_connection.set_local_description(answer).await?;

    let _ = gather_complete_rx.recv().await;

    if let Some(local_desc) = peer_connection.local_description().await {
        println!("answer: {}", local_desc);
        let json_str = serde_json::to_string(&local_desc)
            .map_err(|e| webrtc::error::Error::Other(e.to_string()))?;
        let b64 = signal::encode(&json_str);
        println!("{b64}");
    } else {
        println!("generate local_description failed!");
    }

    // Spawn a stats printing task
    let pc_stats = peer_connection.clone();
    let track_map = track_id_to_codec.clone();
    runtime.spawn(Box::pin(async move {
        let mut stats_ticker = interval(STATS_INTERVAL);
        loop {
            let _ = stats_ticker.tick().await;

            let report = pc_stats
                .get_stats(Instant::now(), StatsSelector::None)
                .await;
            if report.is_empty() {
                continue;
            }

            println!("\n=== WebRTC Stats ===");

            // Print peer connection stats
            if let Some(pc_stats) = report.peer_connection() {
                println!("{}", serde_json::to_string_pretty(pc_stats).unwrap());
            }

            // Print inbound RTP stream stats
            for inbound_stats in report.inbound_rtp_streams() {
                let map = track_map.lock().await;
                let codec = map
                    .get(&inbound_stats.track_identifier)
                    .map(|s| s.as_str())
                    .unwrap_or("unknown");
                println!("\nInbound RTP Stats for: {codec}");
                println!("{}", serde_json::to_string_pretty(inbound_stats).unwrap());
            }

            // Print ICE candidate stats (only remote candidates)
            for entry in report.iter_by_type(RTCStatsType::RemoteCandidate) {
                if let rtc::statistics::report::RTCStatsReportEntry::RemoteCandidate(cand_stats) =
                    entry
                {
                    println!(
                        "\nRemote Candidate:\n{}",
                        serde_json::to_string_pretty(cand_stats).unwrap()
                    );
                }
            }

            println!("====================\n");
        }
    }));

    println!("Press ctrl-c to stop");
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    std::thread::spawn(move || {
        let mut ctrlc_tx = Some(ctrlc_tx);
        ctrlc::set_handler(move || {
            if let Some(tx) = ctrlc_tx.take() {
                let _ = tx.try_send(());
            }
        })
        .expect("Error setting Ctrl-C handler");
    });

    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("received done signal!");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!("received ctrl-c signal!");
        }
    };

    peer_connection.close().await?;

    Ok(())
}
