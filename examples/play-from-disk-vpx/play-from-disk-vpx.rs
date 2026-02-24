use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::media::Sample;
use rtc::media::io::ivf_reader::IVFReader;
use rtc::media::io::ogg_reader::OggReader;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{
    MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9, MediaEngine,
};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RtpCodecKind};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::{
    fs,
    fs::{File, OpenOptions},
    io::{BufReader, Write},
    str::FromStr,
};
use webrtc::media_stream::Track;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Sender, block_on, channel, default_runtime, interval};

const OGG_PAGE_DURATION: Duration = Duration::from_millis(20);

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "play-from-disk-vpx")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "An example of play-from-disk-vpx.")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    /// Video file to stream (.ivf containing VP8 or VP9)
    #[arg(short, long)]
    video: Option<String>,
    /// Audio file to stream (.ogg / Opus)
    #[arg(short, long)]
    audio: Option<String>,
    /// Use VP9 instead of VP8 (default: VP8)
    #[arg(long)]
    vp9: bool,
}

// ── Event handler ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Handler {
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    connected_tx: Sender<()>,
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
                let _ = self.connected_tx.try_send(());
            }
            RTCPeerConnectionState::Failed
            | RTCPeerConnectionState::Disconnected
            | RTCPeerConnectionState::Closed => {
                let _ = self.done_tx.try_send(());
            }
            _ => {}
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
    let is_vp9 = cli.vp9;
    let video_file = cli.video;
    let audio_file = cli.audio;

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

    if let Some(video_path) = &video_file {
        if !Path::new(video_path).exists() {
            return Err(anyhow::anyhow!("video file: '{}' not exist", video_path));
        }
    }
    if let Some(audio_path) = &audio_file {
        if !Path::new(audio_path).exists() {
            return Err(anyhow::anyhow!("audio file: '{}' not exist", audio_path));
        }
    }

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

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
        payload_type: 120,
        ..Default::default()
    };

    let video_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: if is_vp9 {
                MIME_TYPE_VP9.to_owned()
            } else {
                MIME_TYPE_VP8.to_owned()
            },
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: if is_vp9 { 98 } else { 96 },
        ..Default::default()
    };

    if audio_file.is_some() {
        media_engine.register_codec(audio_codec.clone(), RtpCodecKind::Audio)?;
    }
    if video_file.is_some() {
        media_engine.register_codec(video_codec.clone(), RtpCodecKind::Video)?;
    }

    let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;

    // Create RTC peer connection configuration
    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    let (done_tx, mut done_rx) = channel::<()>(1);
    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>(1);
    let (connected_tx, mut connected_rx) = channel::<()>(1);
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let handler = Arc::new(Handler {
        gather_complete_tx,
        done_tx: done_tx.clone(),
        connected_tx,
    });

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
        .build()
        .await?;

    // Add video track
    let video_track: Option<Arc<TrackLocalStaticSample>> = if video_file.is_some() {
        let ssrc = rand::random::<u32>();
        let track: Arc<TrackLocalStaticSample> =
            Arc::new(TrackLocalStaticSample::new(MediaStreamTrack::new(
                format!("webrtc-rs-stream-id-{}", RtpCodecKind::Video),
                format!("webrtc-rs-track-id-{}", RtpCodecKind::Video),
                format!("webrtc-rs-track-label-{}", RtpCodecKind::Video),
                RtpCodecKind::Video,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(ssrc),
                        ..Default::default()
                    },
                    codec: video_codec.rtp_codec.clone(),
                    ..Default::default()
                }],
            ))?);
        peer_connection
            .add_track(Arc::clone(&track) as Arc<dyn TrackLocal>)
            .await?;
        Some(track)
    } else {
        None
    };

    // Add audio track
    let audio_track: Option<Arc<TrackLocalStaticSample>> = if audio_file.is_some() {
        let ssrc = rand::random::<u32>();
        let track: Arc<TrackLocalStaticSample> =
            Arc::new(TrackLocalStaticSample::new(MediaStreamTrack::new(
                format!("webrtc-rs-stream-id-{}", RtpCodecKind::Audio),
                format!("webrtc-rs-track-id-{}", RtpCodecKind::Audio),
                format!("webrtc-rs-track-label-{}", RtpCodecKind::Audio),
                RtpCodecKind::Audio,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(ssrc),
                        ..Default::default()
                    },
                    codec: audio_codec.rtp_codec.clone(),
                    ..Default::default()
                }],
            ))?);
        peer_connection
            .add_track(Arc::clone(&track) as Arc<dyn TrackLocal>)
            .await?;
        Some(track)
    } else {
        None
    };

    // Wait for the offer to be pasted
    print!("Paste offer from browser and press Enter: ");

    let line = if cli.input_sdp_file.is_empty() {
        signal::must_read_stdin()?
    } else {
        fs::read_to_string(&cli.input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    println!("Offer received: {offer}");

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

    // Wait for connection, then start streaming
    println!("Waiting for peer connection...");
    connected_rx.recv().await;
    println!("Connected! Starting media streams.");

    let (video_done_tx, mut video_done_rx) = channel::<()>(1);
    if let (Some(video_file_name), Some(track)) = (video_file, video_track) {
        runtime.spawn(Box::pin(async move {
            if let Err(e) = stream_video(video_file_name, track).await {
                eprintln!("video streaming error: {e}");
            }
            let _ = video_done_tx.try_send(());
        }));
    } else {
        drop(video_done_tx);
    }

    let (audio_done_tx, mut audio_done_rx) = channel::<()>(1);
    if let (Some(audio_file_name), Some(track)) = (audio_file, audio_track) {
        runtime.spawn(Box::pin(async move {
            if let Err(e) = stream_audio(audio_file_name, track).await {
                eprintln!("audio streaming error: {e}");
            }
            let _ = audio_done_tx.try_send(());
        }));
    } else {
        drop(audio_done_tx);
    }

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("received done signal!");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!("received ctrl-c signal!");
        }
        _ = async {
            let _ = video_done_rx.recv().await;
            let _ = audio_done_rx.recv().await;
        }.fuse() => {
            println!("All media streaming completed.");
        }
    }

    peer_connection.close().await?;

    Ok(())
}

// ── Streaming helpers ──────────────────────────────────────────────────────────

async fn stream_video(
    video_file_name: String,
    video_track: Arc<TrackLocalStaticSample>,
) -> Result<()> {
    // Open a IVF file and start reading using our IVFReader
    let file = File::open(&video_file_name)?;
    let reader = BufReader::new(file);
    let (mut ivf, header) = IVFReader::new(reader)?;

    println!("play video from disk file {video_file_name}");

    let ssrc = video_track.track().ssrcs().next().unwrap_or(0);

    // It is important to use a time.Ticker instead of time.Sleep because
    // * avoids accumulating skew, just calling time.Sleep didn't compensate for the time spent parsing the data
    // * works around latency issues with Sleep
    // Send our video file frame at a time. Pace our sending so we send it at the same speed it should be played back as.
    // This isn't required since the video is timestamped, but we will such much higher loss if we send all at once.
    let sleep_time = Duration::from_millis(
        ((1000 * header.timebase_numerator) / header.timebase_denominator) as u64,
    );
    let mut ticker = interval(sleep_time);
    loop {
        let frame = match ivf.parse_next_frame() {
            Ok((frame, _)) => frame,
            Err(err) => {
                println!("All video frames parsed and sent: {err}");
                break;
            }
        };

        video_track
            .sample_writer(ssrc)
            .write_sample(&Sample {
                data: frame.freeze(),
                duration: Duration::from_secs(1),
                ..Default::default()
            })
            .await?;

        let _ = ticker.tick().await;
    }

    Ok(())
}

async fn stream_audio(
    audio_file_name: String,
    audio_track: Arc<TrackLocalStaticSample>,
) -> Result<()> {
    let file = File::open(&audio_file_name)?;
    let reader = BufReader::new(file);
    let (mut ogg, _) = match OggReader::new(reader, true) {
        Ok(tup) => tup,
        Err(err) => {
            println!("error while opening audio file {audio_file_name}: {err}");
            return Err(err.into());
        }
    };

    println!("play audio from disk file {audio_file_name}");
    let ssrc = audio_track.track().ssrcs().next().unwrap_or(0);

    // It is important to use a time.Ticker instead of time.Sleep because
    // * avoids accumulating skew, just calling time.Sleep didn't compensate for the time spent parsing the data
    // * works around latency issues with Sleep
    let mut ticker = interval(OGG_PAGE_DURATION);

    // Keep track of last granule, the difference is the amount of samples in the buffer
    let mut last_granule: u64 = 0;
    while let Ok((page_data, page_header)) = ogg.parse_next_page() {
        let sample_count = page_header.granule_position - last_granule;
        last_granule = page_header.granule_position;
        let sample_duration = Duration::from_millis(sample_count * 1000 / 48000);

        audio_track
            .sample_writer(ssrc)
            .write_sample(&Sample {
                data: page_data.freeze(),
                duration: sample_duration,
                ..Default::default()
            })
            .await?;

        let _ = ticker.tick().await;
    }

    Ok(())
}
