use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::{Interceptor, Packet, Registry, StreamInfo, TaggedPacket, interceptor};
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{
    MIME_TYPE_OPUS, MIME_TYPE_VP8, MediaEngine,
};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use rtc::sansio;
use rtc::shared::error::Error;
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, block_on, channel, default_runtime};

// ============================================================================
// RTCP Forwarder Interceptor
// ============================================================================
//
// This interceptor forwards RTCP packets to the application via poll_read().
// By default, RTCP packets are consumed by the interceptor chain (for generating
// statistics, NACK, etc.) and not forwarded to the application.

/// Builder for the RtcpForwarderInterceptor.
pub struct RtcpForwarderBuilder<P> {
    _phantom: std::marker::PhantomData<P>,
}

impl<P> Default for RtcpForwarderBuilder<P> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<P> RtcpForwarderBuilder<P> {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the interceptor.
    pub fn build(self) -> impl FnOnce(P) -> RtcpForwarderInterceptor<P> {
        move |inner| RtcpForwarderInterceptor::new(inner)
    }
}

/// Interceptor that forwards RTCP packets to the application.
///
/// This interceptor intercepts incoming RTCP packets and queues them for
/// `poll_read()`, allowing the application to receive and process RTCP packets.
#[derive(Interceptor)]
pub struct RtcpForwarderInterceptor<P> {
    #[next]
    next: P,
    read_queue: VecDeque<TaggedPacket>,
}

impl<P> RtcpForwarderInterceptor<P> {
    /// Create a new RtcpForwarderInterceptor.
    fn new(next: P) -> Self {
        Self {
            next,
            read_queue: VecDeque::new(),
        }
    }
}

#[interceptor]
impl<P: Interceptor> RtcpForwarderInterceptor<P> {
    #[overrides]
    fn handle_read(&mut self, msg: TaggedPacket) -> Result<(), Self::Error> {
        // If this is an RTCP packet, queue a copy for the application
        if let Packet::Rtcp(rtcp_packets) = &msg.message {
            self.read_queue.push_back(TaggedPacket {
                now: msg.now,
                transport: msg.transport,
                message: Packet::Rtcp(rtcp_packets.clone()),
            });
        }
        // Always pass to next interceptor for normal processing
        self.next.handle_read(msg)
    }

    #[overrides]
    fn poll_read(&mut self) -> Option<Self::Rout> {
        // First return any queued RTCP packets
        if let Some(pkt) = self.read_queue.pop_front() {
            return Some(pkt);
        }
        // Then check next interceptor
        self.next.poll_read()
    }

    #[overrides]
    fn close(&mut self) -> Result<(), Self::Error> {
        self.read_queue.clear();
        self.next.close()
    }
}

// ============================================================================
// Main Application
// ============================================================================

#[derive(Parser)]
#[command(name = "rtcp-processing")]
#[command(author = "Rusty Rain <y@liu.mx>")]
#[command(version = "0.1.0")]
#[command(about = "An example of RTCP packet processing")]
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
struct Handler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    rtcp_count: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        println!("ICE gathering state: {:?}", state);
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer Connection State has changed: {state}");
        match state {
            RTCPeerConnectionState::Connected => {
                println!("Connection established! Waiting for RTCP packets...\n");
            }
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
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

        let track_id = track.track_id().await;
        let stream_id = track.stream_id().await;
        let kind = track.kind().await;
        let codec = track.codec(media_ssrc).await;

        println!("Track has started - track_id: {track_id}");
        if let Some(codec) = codec {
            println!(
                "  Stream ID: {}, Track ID: {}, Kind: {}, Codec: {}",
                stream_id, track_id, kind, codec.mime_type
            );
        } else {
            println!(
                "  Stream ID: {}, Track ID: {}, Kind: {}",
                stream_id, track_id, kind
            );
        }
        println!();

        let rtcp_count = Arc::clone(&self.rtcp_count);
        self.runtime.spawn(Box::pin(async move {
            while let Some(evt) = track.poll().await {
                match evt {
                    TrackRemoteEvent::OnRtcpPacket(rtcp_packets) => {
                        let batch = rtcp_count.fetch_add(1, Ordering::Relaxed) + 1;
                        println!("=== RTCP Packet #{} (Track: {}) ===", batch, track_id);

                        for (i, packet) in rtcp_packets.iter().enumerate() {
                            let header = packet.header();
                            println!(
                                "  [{}] Type: {:?}, Length: {} words",
                                i + 1,
                                header.packet_type,
                                header.length
                            );

                            for line in format!("{packet}").lines() {
                                println!("      {}", line);
                            }
                        }
                        println!();
                    }
                    TrackRemoteEvent::OnEnded => {
                        println!("Track closed: {}", track_id);
                        break;
                    }
                    TrackRemoteEvent::OnError => {
                        eprintln!("Track error: {}", track_id);
                        break;
                    }
                    _ => {}
                }
            }
        }));
    }
}

fn main() -> anyhow::Result<()> {
    block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let input_sdp_file = cli.input_sdp_file;
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

    // Run the peer connection with event loop
    run(input_sdp_file).await?;

    Ok(())
}

async fn run(input_sdp_file: String) -> anyhow::Result<()> {
    let mut media_engine = MediaEngine::default();

    // Register VP8 codec for video
    media_engine.register_codec(
        RTCRtpCodecParameters {
            rtp_codec: RTCRtpCodec {
                mime_type: MIME_TYPE_VP8.to_string(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_string(),
                rtcp_feedback: vec![],
            },
            payload_type: 96,
            ..Default::default()
        },
        RtpCodecKind::Video,
    )?;

    // Register Opus codec for audio
    media_engine.register_codec(
        RTCRtpCodecParameters {
            rtp_codec: RTCRtpCodec {
                mime_type: MIME_TYPE_OPUS.to_string(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "".to_string(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        },
        RtpCodecKind::Audio,
    )?;

    // Create interceptor registry with RTCP forwarder
    let registry = Registry::new();

    // Register default interceptors (NACK, reports, etc.)
    let registry = register_default_interceptors(registry, &mut media_engine)?;

    // Add our RTCP forwarder interceptor as the outermost layer
    // This ensures RTCP packets are captured before being consumed
    let registry = registry.with(RtcpForwarderBuilder::new().build());

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
    let rtcp_count = Arc::new(AtomicU64::new(0));

    let handler = Arc::new(Handler {
        runtime: runtime.clone(),
        gather_complete_tx,
        done_tx: done_tx.clone(),
        rtcp_count: Arc::clone(&rtcp_count),
    });

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(Arc::clone(&handler) as Arc<dyn PeerConnectionEventHandler>)
        .with_runtime(runtime)
        .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
        .build()
        .await?;

    peer_connection
        .add_transceiver_from_kind(
            RtpCodecKind::Audio,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                ..Default::default()
            }),
        )
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
    let line = if input_sdp_file.is_empty() {
        signal::must_read_stdin()?
    } else {
        std::fs::read_to_string(&input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    println!("Offer received: {}", offer);

    peer_connection.set_remote_description(offer).await?;
    let answer = peer_connection.create_answer(None).await?;
    peer_connection.set_local_description(answer).await?;

    let _ = gather_complete_rx.recv().await;

    if let Some(local_desc) = peer_connection.local_description().await {
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = signal::encode(&json_str);
        println!("\nPaste this answer in your browser:\n{}\n", b64);
    } else {
        println!("generate local_description failed!");
    }

    println!("Waiting for RTCP packets...");
    println!("Press Ctrl-C to stop\n");

    futures::select! {
        _ = done_rx.recv().fuse() => {}
        _ = ctrlc_rx.recv().fuse() => {
            println!("\nCtrl-C received, shutting down...");
        }
    }

    println!(
        "Total RTCP packets received: {}",
        rtcp_count.load(Ordering::Relaxed)
    );
    peer_connection.close().await?;
    println!("Event loop exited");

    Ok(())
}
