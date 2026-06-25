use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::media::io::Writer;
use rtc::media::io::ivf_reader::IVFFileHeader;
use rtc::media::io::ivf_writer::IVFWriter;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_AV1, MediaEngine};
use rtc::peer_connection::configuration::setting_engine::SettingEngine;
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCDtlsRole;
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

#[derive(Parser)]
#[command(name = "save-to-disk-av1")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "An example of save-to-disk-av1.")]
struct Cli {
    #[arg(short, long)]
    client: bool,
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    #[arg(long, default_value_t = format!("0.0.0.0"))]
    host: String,
    #[arg(long, default_value_t = 0)]
    port: u16,
    #[arg(short, long, default_value_t = format!("output.ivf"))]
    video: String,
}

#[derive(Clone)]
struct Handler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    video_writer: Arc<std::sync::Mutex<Option<IVFWriter<File>>>>,
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
                println!("Done writing media files");
                let _ = self.done_tx.try_send(());
            }
            _ => {}
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let Some(media_ssrc) = track.ssrcs().await.first().copied() else {
            eprintln!("track exposed no SSRCs");
            return;
        };
        let mime_type = track
            .codec(media_ssrc)
            .await
            .map(|c| c.mime_type.to_lowercase())
            .unwrap_or_default();

        if mime_type != MIME_TYPE_AV1.to_lowercase() {
            return;
        }

        println!("Got AV1 track, saving to disk");

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

        let video_writer = Arc::clone(&self.video_writer);
        self.runtime.spawn(Box::pin(async move {
            while let Some(evt) = track.poll().await {
                if let TrackRemoteEvent::OnRtpPacket(packet) = evt {
                    let mut guard = video_writer.lock().unwrap();
                    if let Some(ref mut writer) = *guard {
                        if let Err(err) = writer.write_rtp(&packet) {
                            println!("video write_rtp error: {err}");
                            break;
                        }
                    }
                }
            }

            let mut guard = video_writer.lock().unwrap();
            if let Some(mut writer) = guard.take() {
                if let Err(err) = writer.close() {
                    println!("Error closing video file: {err}");
                }
            }
            println!("Video track ended, file closed.");
        }));
    }
}

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

    let video_writer = Arc::new(std::sync::Mutex::new(Some(IVFWriter::new(
        File::create(&cli.video)?,
        &IVFFileHeader {
            signature: *b"DKIF",
            version: 0,
            header_size: 32,
            four_cc: *b"AV01",
            width: 640,
            height: 480,
            timebase_denominator: 30,
            timebase_numerator: 1,
            num_frames: 900,
            unused: 0,
        },
    )?)));

    let mut setting_engine = SettingEngine::default();
    setting_engine.set_answering_dtls_role(if cli.client {
        RTCDtlsRole::Client
    } else {
        RTCDtlsRole::Server
    })?;

    let mut media_engine = MediaEngine::default();
    media_engine.register_codec(
        RTCRtpCodecParameters {
            rtp_codec: RTCRtpCodec {
                mime_type: MIME_TYPE_AV1.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
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
    });

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_setting_engine(setting_engine)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(Arc::clone(&handler) as Arc<dyn PeerConnectionEventHandler>)
        .with_runtime(runtime)
        .with_udp_addrs(vec![format!("{}:{}", cli.host, cli.port)])
        .build()
        .await?;

    peer_connection
        .add_transceiver_from_kind(
            RtpCodecKind::Video,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                ..Default::default()
            }),
        )
        .await?;

    print!("Paste offer from browser and press Enter: ");
    let line = if cli.input_sdp_file.is_empty() {
        signal::must_read_stdin()?
    } else {
        fs::read_to_string(&cli.input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    println!("Offer received: {}", offer);

    peer_connection.set_remote_description(offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    peer_connection.set_local_description(answer).await?;

    let _ = gather_complete_rx.recv().await;

    if let Some(local_desc) = peer_connection.local_description().await {
        println!("answer created: {}", local_desc);
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = signal::encode(&json_str);
        println!("{b64}");
    } else {
        println!("generate local_description failed!");
    }

    println!("listening for AV1 on {}:{}...", cli.host, cli.port);
    println!("Press ctrl-c to stop");

    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("received done signal!");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!("received ctrl-c signal!");
        }
    }

    let mut guard = video_writer.lock().unwrap();
    if let Some(mut writer) = guard.take() {
        println!("Closing video file");
        if let Err(err) = writer.close() {
            println!("Error closing video file: {err}");
        }
    }

    peer_connection.close().await?;

    Ok(())
}
