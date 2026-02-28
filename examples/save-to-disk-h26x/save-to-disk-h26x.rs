use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::media::io::Writer;
use rtc::media::io::h26x_writer::H26xWriter;
use rtc::media::io::ogg_writer::OggWriter;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{
    MIME_TYPE_H264, MIME_TYPE_HEVC, MIME_TYPE_OPUS, MediaEngine,
};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use rtc::rtp_transceiver::RTCRtpTransceiverDirection;
use rtc::rtp_transceiver::RTCRtpTransceiverInit;
use rtc::rtp_transceiver::rtp_sender::RTCRtpCodecParameters;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RtpCodecKind};
use std::fs::File;
use std::sync::Arc;
use std::time::Duration;
use std::{fs, fs::OpenOptions, io::Write as IoWrite, str::FromStr};
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "save-to-disk-h26x")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "An example of save-to-disk-h26x.")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    /// H264/H265 output video file
    #[arg(short, long)]
    video: Option<String>,
    /// Opus OGG output audio file
    #[arg(short, long)]
    audio: Option<String>,
    /// Save H265/HEVC instead of H264 (default: H264)
    #[arg(long)]
    hevc: bool,
}

// ── Event handler ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Handler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    video_writer: Arc<std::sync::Mutex<Option<H26xWriter<File>>>>,
    audio_writer: Arc<std::sync::Mutex<Option<OggWriter<File>>>>,
    is_hevc: bool,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer Connection State has changed: {state}");
        match state {
            RTCPeerConnectionState::Connected => {
                println!("Ctrl+C the remote client to stop the demo");
            }
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                let _ = self.done_tx.try_send(());
            }
            _ => {}
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let media_ssrc = track.track().ssrcs().next().unwrap();
        let kind = track.track().kind();
        let mime_type = track
            .track()
            .codec(media_ssrc)
            .map(|c| c.mime_type.to_lowercase())
            .unwrap_or_default();

        // Send PLI every 3 seconds for video tracks to request keyframes
        if kind == RtpCodecKind::Video {
            let pli_track = track.clone();
            self.runtime.spawn(Box::pin(async move {
                let mut result = webrtc::error::Result::<()>::Ok(());
                while result.is_ok() {
                    let timeout = sleep(Duration::from_secs(3));
                    futures::pin_mut!(timeout);
                    futures::select! {
                        _ = timeout.fuse() => {
                            result = pli_track
                                .write_rtcp(vec![Box::new(PictureLossIndication {
                                    sender_ssrc: 0,
                                    media_ssrc,
                                })])
                                .await;
                        }
                    }
                }
            }));
        }

        let is_hevc = self.is_hevc;
        if mime_type == MIME_TYPE_OPUS.to_lowercase() {
            println!("Got Opus track, saving to disk (48 kHz, 2 channels)");
            let audio_writer = Arc::clone(&self.audio_writer);
            self.runtime.spawn(Box::pin(async move {
                while let Some(evt) = track.poll().await {
                    if let TrackRemoteEvent::OnRtpPacket(packet) = evt {
                        let mut guard = audio_writer.lock().unwrap();
                        if let Some(ref mut w) = *guard {
                            if let Err(err) = w.write_rtp(&packet) {
                                println!("audio write_rtp error: {err}");
                                break;
                            }
                        }
                    }
                }
                let mut guard = audio_writer.lock().unwrap();
                if let Some(ref mut w) = *guard {
                    if let Err(err) = w.close() {
                        println!("audio file close error: {err}");
                    }
                }
                println!("Audio track ended, file closed.");
            }));
        } else if mime_type == MIME_TYPE_H264.to_lowercase()
            || mime_type == MIME_TYPE_HEVC.to_lowercase()
        {
            println!(
                "Got {} track, saving to disk",
                if is_hevc { "H265" } else { "H264" }
            );
            let video_writer = Arc::clone(&self.video_writer);
            self.runtime.spawn(Box::pin(async move {
                while let Some(evt) = track.poll().await {
                    if let TrackRemoteEvent::OnRtpPacket(packet) = evt {
                        let mut guard = video_writer.lock().unwrap();
                        if let Some(ref mut w) = *guard {
                            if let Err(err) = w.write_rtp(&packet) {
                                println!("video write_rtp error: {err}");
                                break;
                            }
                        }
                    }
                }
                let mut guard = video_writer.lock().unwrap();
                if let Some(ref mut w) = *guard {
                    if let Err(err) = w.close() {
                        println!("video file close error: {err}");
                    }
                }
                println!("Video track ended, file closed.");
            }));
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    block_on(async_main())
}

async fn async_main() -> Result<()> {
    let cli = Cli::parse();
    let log_level = log::LevelFilter::from_str(&cli.log_level)?;
    let is_hevc = cli.hevc;

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

    // Create file writers up front
    let video_writer: Arc<std::sync::Mutex<Option<H26xWriter<File>>>> =
        Arc::new(std::sync::Mutex::new(if let Some(ref path) = cli.video {
            Some(H26xWriter::new(File::create(path)?, is_hevc))
        } else {
            None
        }));

    let audio_writer: Arc<std::sync::Mutex<Option<OggWriter<File>>>> =
        Arc::new(std::sync::Mutex::new(if let Some(ref path) = cli.audio {
            Some(OggWriter::new(File::create(path)?, 48000, 2)?)
        } else {
            None
        }));

    // Create a MediaEngine object to configure the supported codec
    let mut media_engine = MediaEngine::default();

    let audio_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: 48000,
            channels: 2,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 111,
        ..Default::default()
    };

    let video_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: if is_hevc {
                MIME_TYPE_HEVC.to_owned()
            } else {
                MIME_TYPE_H264.to_owned()
            },
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: if is_hevc {
                "".to_owned()
            } else {
                "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f".to_owned()
            },
            rtcp_feedback: vec![],
        },
        payload_type: if is_hevc { 98 } else { 102 },
        ..Default::default()
    };

    if cli.audio.is_some() {
        media_engine.register_codec(audio_codec, RtpCodecKind::Audio)?;
    }
    if cli.video.is_some() {
        media_engine.register_codec(video_codec, RtpCodecKind::Video)?;
    }

    let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;

    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    let (done_tx, mut done_rx) = channel::<()>(1);
    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>(1);
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let handler = Arc::new(Handler {
        runtime: runtime.clone(),
        gather_complete_tx,
        done_tx: done_tx.clone(),
        video_writer: Arc::clone(&video_writer),
        audio_writer: Arc::clone(&audio_writer),
        is_hevc,
    });

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(Arc::clone(&handler) as Arc<dyn PeerConnectionEventHandler>)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
        .build()
        .await?;

    // Allow us to receive 1 audio track, and 1 video track
    if cli.audio.is_some() {
        peer_connection
            .add_transceiver_from_kind(
                RtpCodecKind::Audio,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    ..Default::default()
                }),
            )
            .await?;
    }
    if cli.video.is_some() {
        peer_connection
            .add_transceiver_from_kind(
                RtpCodecKind::Video,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    ..Default::default()
                }),
            )
            .await?;
    }

    // Wait for the offer to be pasted
    print!("Paste offer from browser and press Enter: ");
    let line = if cli.input_sdp_file.is_empty() {
        signal::must_read_stdin()?
    } else {
        fs::read_to_string(&cli.input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;

    peer_connection.set_remote_description(offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    peer_connection.set_local_description(answer).await?;

    // Block until ICE Gathering is complete (non-trickle ICE)
    let _ = gather_complete_rx.recv().await;

    if let Some(local_desc) = peer_connection.local_description().await {
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = signal::encode(&json_str);
        println!("{b64}");
    } else {
        println!("generate local_description failed!");
    }

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("received done signal!");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!("received ctrl-c signal!");
        }
    }

    peer_connection.close().await?;

    Ok(())
}
