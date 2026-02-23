use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::Registry;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{
    MIME_TYPE_OPUS, MIME_TYPE_VP8, MediaEngine,
};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RtpCodecKind};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::{fs, fs::OpenOptions, io::Write, str::FromStr};
use webrtc::error::{Error, Result};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Mutex, Runtime, Sender, block_on, channel, default_runtime, sleep};

#[derive(Clone)]
struct TestHandler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    tracks: Arc<Mutex<HashMap<RtpCodecKind, Arc<dyn TrackLocal>>>>,
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
            println!("Peer Connection has gone to failed exiting");
            let _ = self.done_tx.try_send(());
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        // Send a PLI on an interval so that the publisher is pushing a keyframe every rtcpPLIInterval
        // This is a temporary fix until we implement incoming RTCP events, then we would push a PLI only when a viewer requests it
        let media_ssrc = track.track().ssrcs().next().unwrap();

        if track.track().kind() == RtpCodecKind::Video {
            let trace_remote = track.clone();
            self.runtime.spawn(Box::pin(async move {
                let mut result = Result::<()>::Ok(());
                while result.is_ok() {
                    let timeout = sleep(Duration::from_secs(3));
                    futures::pin_mut!(timeout);

                    futures::select! {
                        _ = timeout.fuse() =>{
                            result = trace_remote.write_rtcp(vec![Box::new(PictureLossIndication{
                                    sender_ssrc: 0,
                                    media_ssrc,
                            })]).await;
                        }
                    };
                }
            }));
        }

        let kind = track.track().kind();
        let tracks = self.tracks.lock().await;
        let output_track = if let Some(output_track) = tracks.get(&kind) {
            Arc::clone(output_track)
        } else {
            println!("output_track not found for type = {kind}");
            return;
        };
        self.runtime.spawn(Box::pin(async move {
            println!(
                "Track has started, of mime_type {}",
                track.track().codec(media_ssrc).unwrap().mime_type
            );
            // Read RTP packets being sent to webrtc-rs
            while let Some(evt) = track.poll().await {
                if let TrackRemoteEvent::OnRtpPacket(mut packet) = evt {
                    packet.header.ssrc = output_track
                        .track()
                        .ssrcs()
                        .next()
                        .ok_or(Error::ErrSenderWithNoSSRCs)
                        .unwrap();
                    if let Err(err) = output_track.write_rtp(packet).await {
                        println!("output track write_rtp got error: {err}");
                        break;
                    }
                }
            }

            println!(
                "on_track finished, of mime_type {}",
                track.track().codec(media_ssrc).unwrap().mime_type
            );
        }));
    }
}

#[derive(Parser)]
#[command(name = "reflect")]
#[command(author = "Rusty Rain <y@liu.mx>")]
#[command(version = "0.0.0")]
#[command(
    about = "An example of how to send back to the user exactly what it receives using the same PeerConnection."
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
    #[arg(short, long)]
    audio: bool,
    #[arg(short, long)]
    video: bool,
}
fn main() -> anyhow::Result<()> {
    block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let input_sdp_file = cli.input_sdp_file;
    let output_log_file = cli.output_log_file;
    let log_level = log::LevelFilter::from_str(&cli.log_level)?;
    let audio = cli.audio;
    let video = cli.video;
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
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 96,
        ..Default::default()
    };

    // Setup the codecs you want to use.
    if audio {
        media_engine.register_codec(audio_codec.clone(), RtpCodecKind::Audio)?;
    }

    // We'll use a VP8 and Opus but you can also define your own
    if video {
        media_engine.register_codec(video_codec.clone(), RtpCodecKind::Video)?;
    }

    let registry = Registry::new();

    // Use the default set of Interceptors
    let registry = register_default_interceptors(registry, &mut media_engine)?;

    // Create RTC peer connection configuration
    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    let (done_tx, mut done_rx) = channel::<()>(1);
    let (gather_complete_tx, mut gather_complete_rx) = channel(1);
    let (ctrlc_tx, mut ctrlc_rx) = channel::<()>(1);
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.try_send(());
    })?;
    let runtime =
        default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?;

    let tracks = Arc::new(Mutex::new(HashMap::new()));
    let handler = Arc::new(TestHandler {
        runtime: runtime.clone(),
        gather_complete_tx,
        done_tx,
        tracks: Arc::clone(&tracks),
    });

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_runtime(runtime)
        .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
        .build()
        .await?;

    let mut kind_codecs = HashMap::new();
    if audio {
        kind_codecs.insert(RtpCodecKind::Audio, audio_codec);
    }
    if video {
        kind_codecs.insert(RtpCodecKind::Video, video_codec);
    };

    let mut output_tracks = HashMap::new();
    for (kind, codec) in kind_codecs {
        let output_track: Arc<dyn TrackLocal> =
            Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
                format!("webrtc-rs-stream-id-{}", kind),
                format!("webrtc-rs-track-id-{}", kind),
                format!("webrtc-rs-track-label-{}", kind),
                kind,
                vec![RTCRtpEncodingParameters {
                    rtp_coding_parameters: RTCRtpCodingParameters {
                        ssrc: Some(rand::random::<u32>()),
                        ..Default::default()
                    },
                    codec: codec.rtp_codec.clone(),
                    ..Default::default()
                }],
            )));

        // Add this newly created track to the PeerConnection
        let _rtp_sender = peer_connection.add_track(Arc::clone(&output_track)).await?;

        // Read incoming RTCP packets
        // Before these packets are returned they are processed by interceptors. For things
        // like NACK this needs to be called.
        /*TODO: let m = s.to_owned();
        tokio::spawn(async move {
            let mut rtcp_buf = vec![0u8; 1500];
            while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
            println!("{m} rtp_sender.read loop exit");
            Result::<()>::Ok(())
        });*/

        output_tracks.insert(kind, output_track);
    }
    {
        let mut tracks = tracks.lock().await;
        *tracks = output_tracks;
    }

    // Wait for the offer to be pasted
    print!("Paste offer from browser and press Enter: ");

    let line = if input_sdp_file.is_empty() {
        signal::must_read_stdin()?
    } else {
        fs::read_to_string(input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    println!("Offer received: {}", offer);

    // Set the remote SessionDescription
    peer_connection.set_remote_description(offer).await?;

    // Create an answer
    let answer = peer_connection.create_answer(None).await?;

    // Sets the LocalDescription, and starts our UDP listeners
    peer_connection.set_local_description(answer).await?;

    // Block until ICE Gathering is complete, disabling trickle ICE
    // we do this because we only can exchange one signaling message
    // in a production application you should exchange ICE Candidates via OnICECandidate
    let _ = gather_complete_rx.recv().await;

    // Output the answer in base64 so we can paste it in browser
    if let Some(local_desc) = peer_connection.local_description().await {
        println!("answer created: {}", local_desc);
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
    };

    peer_connection.close().await?;

    Ok(())
}
