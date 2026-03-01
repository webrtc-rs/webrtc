use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use log::trace;
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
    RTCRtpHeaderExtensionCapability, RtpCodecKind,
};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use std::collections::HashMap;
use std::sync::Arc;
use std::{fs, fs::OpenOptions, io::Write as IoWrite, str::FromStr};
use webrtc::error::Result as WebRtcResult;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime, sleep};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "simulcast")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "An example of simulcast: receive 3 simulcast layers and echo them back.")]
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
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    // rid -> (output_track, output_ssrc)
    output_tracks: Arc<HashMap<String, (Arc<TrackLocalStaticRTP>, u32)>>,
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
        if state == RTCPeerConnectionState::Failed || state == RTCPeerConnectionState::Closed {
            let _ = self.done_tx.try_send(());
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        println!("Track has started");

        // Forward RTP to the output track matching each packet's rid.
        // All simulcast layers come through the same TrackRemote.
        // OnOpen fires once per simulcast RID (carrying the correct SSRC), so we
        // eagerly build the ssrc→output_track cache and spawn PLI tasks there.
        let output_tracks = self.output_tracks.clone();
        let runtime = self.runtime.clone();
        self.runtime.spawn(Box::pin(async move {
            // ssrc → (output_track, output_ssrc), populated at OnOpen time per RID
            let mut ssrc_map: HashMap<u32, (Arc<TrackLocalStaticRTP>, u32)> = HashMap::new();

            while let Some(evt) = track.poll().await {
                match evt {
                    TrackRemoteEvent::OnOpen(init) => {
                        let ssrc = init.ssrc;
                        let rid = init.rid.as_deref().unwrap_or("");
                        println!("Simulcast track opened with rid={rid}, ssrc={ssrc}");

                        if let Some(entry) = output_tracks.get(rid).cloned() {
                            ssrc_map.insert(ssrc, entry.clone());

                            // Spawn a PLI task for this RID using the correct SSRC
                            let pli_track = track.clone();
                            runtime.spawn(Box::pin(async move {
                                let mut result = WebRtcResult::<()>::Ok(());
                                while result.is_ok() {
                                    let timeout = sleep(std::time::Duration::from_secs(3));
                                    futures::pin_mut!(timeout);
                                    futures::select! {
                                        _ = timeout.fuse() => {
                                            result = pli_track
                                                .write_rtcp(vec![Box::new(
                                                    PictureLossIndication {
                                                        sender_ssrc: 0,
                                                        media_ssrc: ssrc,
                                                    },
                                                )])
                                                .await;
                                        }
                                    }
                                }
                            }));
                        } else {
                            println!("No output track for rid={rid}");
                        }
                    }
                    TrackRemoteEvent::OnRtpPacket(mut packet) => {
                        let ssrc = packet.header.ssrc;
                        if let Some((output_track, out_ssrc)) = ssrc_map.get(&ssrc) {
                            trace!(
                                "forwarding rtp ssrc={ssrc} seq={} -> out_ssrc={out_ssrc}",
                                packet.header.sequence_number
                            );
                            packet.header.ssrc = *out_ssrc;
                            if let Err(err) = output_track.write_rtp(packet).await {
                                println!("output track write_rtp got error: {err}");
                                break;
                            }
                        } else {
                            trace!(
                                "OnRtpPacket ssrc={ssrc} not in ssrc_map (keys: {:?})",
                                ssrc_map.keys().collect::<Vec<_>>()
                            );
                        }
                    }
                    _ => {}
                }
            }
            println!("Track forwarding ended.");
        }));
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

    // ── Media engine setup ───────────────────────────────────────────────────

    let mut media_engine = MediaEngine::default();

    // Enable VP8 codec for video (matching the sansio RTC simulcast example)
    let video_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 96,
        ..Default::default()
    };

    media_engine.register_codec(video_codec.clone(), RtpCodecKind::Video)?;

    // Enable extension headers needed for simulcast
    for uri in [
        "urn:ietf:params:rtp-hdrext:sdes:mid",
        "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id",
        "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id",
    ] {
        media_engine.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: uri.to_owned(),
            },
            RtpCodecKind::Video,
            None,
        )?;
    }

    let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;

    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    // ── Output tracks (one per simulcast layer) ──────────────────────────────

    // Build the output tracks map before building the peer connection

    let mut output_track_map: HashMap<String, (Arc<TrackLocalStaticRTP>, u32)> = HashMap::new();
    let mut tracks_to_add: Vec<Arc<TrackLocalStaticRTP>> = Vec::new();

    for rid in ["q", "h", "f"] {
        let ssrc: u32 = rand::random();
        let track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
            format!("webrtc-rs_{rid}"),
            format!("video_{rid}"),
            format!("video_{rid}"),
            RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    rid: rid.to_string(),
                    ssrc: Some(ssrc),
                    ..Default::default()
                },
                codec: video_codec.rtp_codec.clone(),
                ..Default::default()
            }],
        )));
        output_track_map.insert(rid.to_owned(), (Arc::clone(&track), ssrc));
        tracks_to_add.push(track);
    }

    let output_tracks = Arc::new(output_track_map);

    // ── Peer connection ──────────────────────────────────────────────────────

    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let (done_tx, mut done_rx) = channel::<()>(1);
    let (gather_complete_tx, mut gather_complete_rx) = channel::<()>(1);
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;

    let handler = Arc::new(Handler {
        runtime: runtime.clone(),
        gather_complete_tx,
        done_tx: done_tx.clone(),
        output_tracks,
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

    // Add a recvonly transceiver for the incoming simulcast video
    peer_connection
        .add_transceiver_from_kind(
            RtpCodecKind::Video,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                ..Default::default()
            }),
        )
        .await?;

    // Add output tracks (one sendonly transceiver per simulcast layer)
    for track in tracks_to_add {
        peer_connection
            .add_track(track as Arc<dyn TrackLocal>)
            .await?;
    }

    // ── Signalling ───────────────────────────────────────────────────────────

    print!("Paste offer from browser and press Enter: ");
    let line = if cli.input_sdp_file.is_empty() {
        signal::must_read_stdin()?
    } else {
        fs::read_to_string(&cli.input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    print!("offer: {}", offer);
    peer_connection.set_remote_description(offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    peer_connection.set_local_description(answer).await?;

    // Block until ICE gathering is complete (non-trickle ICE)
    let _ = gather_complete_rx.recv().await;

    if let Some(local_desc) = peer_connection.local_description().await {
        print!("answer: {}", local_desc);
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
