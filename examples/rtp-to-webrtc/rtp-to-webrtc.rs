use anyhow::Result;
use bytes::BytesMut;
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
use rtc::rtp;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use rtc::shared::marshal::Unmarshal;
use std::sync::Arc;
use std::{fs, fs::OpenOptions, io::Write as IoWrite, str::FromStr};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{AsyncUdpSocket, Sender, block_on, channel, default_runtime};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "rtp-to-webrtc")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(
    about = "An example of rtp-to-webrtc: receives VP8 RTP on UDP 5004 and sends to browser."
)]
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

// ── Event handler ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Handler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
    done_tx: Sender<()>,
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
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
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

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

    let mut media_engine = MediaEngine::default();

    let video_codec = RTCRtpCodec {
        mime_type: MIME_TYPE_VP8.to_owned(),
        clock_rate: 90000,
        channels: 0,
        sdp_fmtp_line: "".to_owned(),
        rtcp_feedback: vec![],
    };

    media_engine.register_codec(
        rtc::rtp_transceiver::rtp_sender::RTCRtpCodecParameters {
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
        connected_tx,
        done_tx: done_tx.clone(),
    });

    // Create VP8 video track
    let ssrc = rand::random::<u32>();
    let video_track: Arc<TrackLocalStaticRTP> =
        Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
            format!("webrtc-rs-stream-id-{}", RtpCodecKind::Video),
            format!("webrtc-rs-track-id-{}", RtpCodecKind::Video),
            format!("webrtc-rs-track-label-{}", RtpCodecKind::Video),
            RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(ssrc),
                    ..Default::default()
                },
                codec: video_codec,
                ..Default::default()
            }],
        )));
    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
        .build()
        .await?;

    peer_connection
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal>)
        .await?;

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

    // Open UDP listener for incoming RTP packets
    let std_listener = std::net::UdpSocket::bind("127.0.0.1:5004")?;
    let listener: Arc<dyn AsyncUdpSocket> = runtime.wrap_udp_socket(std_listener)?;
    println!("Listening for RTP on 127.0.0.1:5004");

    // Wait for WebRTC connection, then start forwarding
    println!("Waiting for peer connection...");
    connected_rx.recv().await;
    println!("Connected! Forwarding RTP to browser.");

    let (fwd_done_tx, mut fwd_done_rx) = channel::<()>(1);
    runtime.spawn(Box::pin(async move {
        let mut buf = vec![0u8; 1600];
        loop {
            match listener.recv_from(&mut buf).await {
                Ok((n, _)) => {
                    let mut bytes = BytesMut::from(&buf[..n]);
                    match rtp::packet::Packet::unmarshal(&mut bytes) {
                        Ok(mut packet) => {
                            // Rewrite SSRC to match what we advertised in the SDP
                            packet.header.ssrc = ssrc;
                            if let Err(err) = video_track.write_rtp(packet).await {
                                eprintln!("write_rtp error: {err}");
                                break;
                            }
                        }
                        Err(err) => {
                            eprintln!("RTP unmarshal error: {err}");
                        }
                    }
                }
                Err(err) => {
                    eprintln!("UDP read error: {err}");
                    break;
                }
            }
        }
        let _ = fwd_done_tx.try_send(());
    }));

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => {
            println!("received done signal!");
        }
        _ = ctrlc_rx.recv().fuse() => {
            println!("received ctrl-c signal!");
        }
        _ = fwd_done_rx.recv().fuse() => {
            println!("RTP forwarding ended.");
        }
    }

    peer_connection.close().await?;

    Ok(())
}
