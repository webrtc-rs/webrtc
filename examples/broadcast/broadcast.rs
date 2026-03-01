use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::MediaEngine;
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use rtc::rtp;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use std::sync::Arc;
use std::{fs::OpenOptions, io::Write, str::FromStr};
use webrtc::error::Result as WebRtcResult;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{
    BroadcastSender, Runtime, Sender, block_on, broadcast_channel, channel, default_runtime, sleep,
};

// ── Broadcaster handler ───────────────────────────────────────────────────────

#[derive(Clone)]
struct BroadcastHandler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    // Sends the broadcaster's codec once when on_track fires
    codec_tx: Sender<RTCRtpCodec>,
    // Broadcast sender: each viewer subscribes to get its own receiver
    broadcast_tx: BroadcastSender<rtp::Packet>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for BroadcastHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Broadcaster Peer Connection State: {state}");
        if state == RTCPeerConnectionState::Failed || state == RTCPeerConnectionState::Closed {
            let _ = self.done_tx.try_send(());
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let media_ssrc = *track.ssrcs().await.first().unwrap();
        let codec = track.codec(media_ssrc).await.unwrap().clone();
        println!(
            "Broadcaster received track: {} ssrc={}",
            codec.mime_type, media_ssrc
        );

        // Publish codec so main loop can create matching viewer tracks
        let _ = self.codec_tx.try_send(codec);

        // Send PLI periodically so the broadcaster keeps pushing keyframes
        let pli_track = track.clone();
        self.runtime.spawn(Box::pin(async move {
            let mut result = WebRtcResult::<()>::Ok(());
            while result.is_ok() {
                let timeout = sleep(std::time::Duration::from_secs(3));
                futures::pin_mut!(timeout);
                futures::select! {
                    _ = timeout.fuse() => {
                        result = pli_track.write_rtcp(vec![Box::new(PictureLossIndication {
                            sender_ssrc: 0,
                            media_ssrc,
                        })]).await;
                    }
                };
            }
        }));

        // Forward RTP packets to all subscribers via broadcast channel
        let broadcast_tx = self.broadcast_tx.clone();
        self.runtime.spawn(Box::pin(async move {
            while let Some(evt) = track.poll().await {
                if let TrackRemoteEvent::OnRtpPacket(packet) = evt {
                    let _ = broadcast_tx.send(packet);
                }
            }
        }));
    }
}

// ── Viewer handler ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct ViewerHandler {
    gather_complete_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for ViewerHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Viewer Peer Connection State: {state}");
    }
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "broadcast")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "An example of broadcast: one sender, multiple viewers.")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    #[arg(long, default_value_t = 8080)]
    port: u16,
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

    let mut sdp_chan_rx = signal::http_sdp_server(cli.port).await;
    println!("Waiting for broadcaster offer on port {}", cli.port);

    // First SDP = broadcaster offer
    let line = sdp_chan_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("SDP channel closed"))?;
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;

    // ── Broadcaster peer connection ──────────────────────────────────────────

    let (done_tx, mut done_rx) = channel::<()>(1);
    let (broadcast_gather_tx, mut broadcast_gather_rx) = channel::<()>(1);
    let (codec_tx, mut codec_rx) = channel::<RTCRtpCodec>(1);
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    // Broadcast sender: each viewer will subscribe to get its own receiver
    let broadcast_tx = broadcast_channel::<rtp::Packet>(256);

    let broadcast_handler = Arc::new(BroadcastHandler {
        runtime: runtime.clone(),
        gather_complete_tx: broadcast_gather_tx,
        done_tx: done_tx.clone(),
        codec_tx,
        broadcast_tx: broadcast_tx.clone(),
    });

    let mut broadcaster_media_engine = MediaEngine::default();
    broadcaster_media_engine.register_default_codecs()?;
    let broadcaster_registry =
        register_default_interceptors(Registry::new(), &mut broadcaster_media_engine)?;
    let broadcaster_config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    let broadcaster_pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(broadcaster_config)
            .with_media_engine(broadcaster_media_engine)
            .with_interceptor_registry(broadcaster_registry)
            .with_handler(broadcast_handler as Arc<dyn PeerConnectionEventHandler>)
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
            .build()
            .await?,
    );

    broadcaster_pc
        .add_transceiver_from_kind(
            RtpCodecKind::Video,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                ..Default::default()
            }),
        )
        .await?;

    broadcaster_pc.set_remote_description(offer).await?;
    let answer = broadcaster_pc.create_answer(None).await?;
    broadcaster_pc.set_local_description(answer).await?;

    let _ = broadcast_gather_rx.recv().await;

    if let Some(local_desc) = broadcaster_pc.local_description().await {
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = signal::encode(&json_str);
        println!("Broadcaster answer:\n{b64}");
    } else {
        anyhow::bail!("generate local_description failed!");
    }

    println!("Waiting for broadcaster track...");
    let broadcaster_codec = codec_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("codec channel closed"))?;
    println!(
        "Broadcaster codec: {}, ready for viewers.",
        broadcaster_codec.mime_type
    );

    // ── Viewer loop ──────────────────────────────────────────────────────────

    let mut viewer_pcs: Vec<Arc<dyn PeerConnection>> = Vec::new();

    loop {
        println!("\nCurl a base64 SDP to start a viewer peer connection");

        futures::select! {
            line_opt = sdp_chan_rx.recv().fuse() => {
                let line = match line_opt {
                    Some(l) => l,
                    None => { println!("SDP channel closed"); break; }
                };

                let desc_data = signal::decode(line.as_str())?;
                let viewer_offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;

                let (viewer_gather_tx, mut viewer_gather_rx) = channel::<()>(1);
                let viewer_handler = Arc::new(ViewerHandler {
                    gather_complete_tx: viewer_gather_tx,
                });

                let mut viewer_media_engine = MediaEngine::default();
                viewer_media_engine.register_default_codecs()?;
                let viewer_registry =
                    register_default_interceptors(Registry::new(), &mut viewer_media_engine)?;
                let viewer_config = RTCConfigurationBuilder::new()
                    .with_ice_servers(vec![RTCIceServer {
                        urls: vec!["stun:stun.l.google.com:19302".to_string()],
                        ..Default::default()
                    }])
                    .build();

                let viewer_pc: Arc<dyn PeerConnection> = Arc::new(
                    PeerConnectionBuilder::new()
                        .with_configuration(viewer_config)
                        .with_media_engine(viewer_media_engine)
                        .with_interceptor_registry(viewer_registry)
                        .with_handler(viewer_handler as Arc<dyn PeerConnectionEventHandler>)
                        .with_runtime(runtime.clone())
                        .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
                        .build()
                        .await?,
                );

                // Each viewer gets its own track with a unique SSRC
                let viewer_ssrc = rand::random::<u32>();
                let viewer_track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
                    format!("webrtc-rs-broadcast-{}", viewer_ssrc),
                    format!("webrtc-rs-broadcast-track-{}", viewer_ssrc),
                    format!("webrtc-rs-broadcast-{}", viewer_ssrc),
                    RtpCodecKind::Video,
                    vec![RTCRtpEncodingParameters {
                        rtp_coding_parameters: RTCRtpCodingParameters {
                            ssrc: Some(viewer_ssrc),
                            ..Default::default()
                        },
                        codec: broadcaster_codec.clone(),
                        ..Default::default()
                    }],
                )));

                viewer_pc
                    .add_track(Arc::clone(&viewer_track) as Arc<dyn TrackLocal>)
                    .await?;

                viewer_pc.set_remote_description(viewer_offer).await?;
                let viewer_answer = viewer_pc.create_answer(None).await?;
                viewer_pc.set_local_description(viewer_answer).await?;

                let _ = viewer_gather_rx.recv().await;

                if let Some(local_desc) = viewer_pc.local_description().await {
                    let json_str = serde_json::to_string(&local_desc)?;
                    let b64 = signal::encode(&json_str);
                    println!("Viewer answer:\n{b64}");
                } else {
                    println!("generate viewer local_description failed!");
                }

                // Each viewer subscribes to the broadcast channel and gets its own receiver
                let mut viewer_rx = broadcast_tx.subscribe();

                // Spawn a per-viewer task: reads packets and writes to this viewer's track
                runtime.spawn(Box::pin(async move {
                    loop {
                        match viewer_rx.recv().await {
                            Ok(mut packet) => {
                                packet.header.ssrc = viewer_ssrc;
                                if let Err(err) = viewer_track.write_rtp(packet).await {
                                    println!("viewer write_rtp error: {err}");
                                    break;
                                }
                            }
                            Err(_) => break, // closed or lagged
                        }
                    }
                }));

                viewer_pcs.push(viewer_pc);
                println!("Total active viewer connections: {}", viewer_pcs.len());
            }
            _ = done_rx.recv().fuse() => {
                println!("Broadcaster disconnected, shutting down.");
                break;
            }
            _ = ctrlc_rx.recv().fuse() => {
                println!("Received ctrl-c, shutting down.");
                break;
            }
        }
    }

    for pc in &viewer_pcs {
        let _ = pc.close().await;
    }
    let _ = broadcaster_pc.close().await;

    Ok(())
}
