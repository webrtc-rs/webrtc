//! bandwidth-estimation-from-disk: switch between three pre-encoded renditions as the estimate moves.
//!
//! A port of pion's `bandwidth-estimation-from-disk`. Send-side congestion control produces one
//! number — how many bits per second the path looks willing to carry — and it is the sender's job
//! to meet it. This example meets it the crudest way that works: three IVF files encoded at 300
//! kbps, 1 Mbps and 2.5 Mbps, and a switch to whichever one fits.
//!
//! # Getting the estimate out
//!
//! The estimator is a plain object behind [`BandwidthEstimator`], but `configure_congestion_control`
//! takes it by value and it ends up boxed inside the interceptor chain, where the application
//! cannot reach it. So the number has to be pushed rather than pulled: [`ReportingEstimator`] wraps
//! the real estimator, delegates every call, and publishes the target where the streaming task can
//! read it.
//!
//! That wrapper is the whole of the integration, and it is worth noticing what it is *not*. There
//! is no callback registration, no event variant, no new peer-connection API — a
//! `BandwidthEstimator` is a function from acknowledgements to a number, and anything that wants to
//! observe that number can sit in the same place the algorithm does.
//!
//! The handoff is an `AtomicU64` holding `f64::to_bits`, not a `watch` channel. These examples are
//! runtime-agnostic — the same source runs under `runtime-tokio` and `runtime-smol` — so reaching
//! for `tokio::sync::watch` would tie the example to one of them. A relaxed atomic is enough for a
//! value that is written by one task and sampled by another, where a reader that misses an update
//! simply uses the previous estimate for one more frame.

use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::{BandwidthEstimator, EstimatorStats, Gcc, PacketReport, Registry};
use rtc::media::Sample;
use rtc::media::io::ivf_reader::{IVFFrameHeader, IVFReader};
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::{
    CongestionFeedback, configure_congestion_control, register_default_interceptors,
};
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_VP8, MediaEngine};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};
use rtc::rtp_transceiver::{PayloadType, SSRC};
use std::io::{Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::{
    fs,
    fs::{File, OpenOptions},
    io::{BufReader, Write as IoWrite},
    str::FromStr,
};
use webrtc::error::Error;
use webrtc::media_stream::Track;
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::rtp_transceiver::RtpSender;
use webrtc::runtime::{Sender, channel};

#[path = "../common/mod.rs"]
mod common;
use common::{block_on, interval, runtime};

/// The IVF file header, which `reset_reader` does not re-parse — a reader handed to it must
/// already be positioned past it.
const IVF_HEADER_SIZE: u64 = 32;

/// The renditions, cheapest first. Each entry is the file and the bitrate it was encoded at; the
/// estimate is compared against these numbers to decide which one to send.
const QUALITY_LEVELS: [(&str, f64); 3] = [
    ("low.ivf", 300_000.0),
    ("med.ivf", 1_000_000.0),
    ("high.ivf", 2_500_000.0),
];

/// Where the estimator starts. The lowest rendition, so the first seconds of the call are
/// deliverable on a path that turns out to be poor, and probing climbs from there. Starting at the
/// highest instead would open the call by congesting the path it is still measuring.
const INITIAL_BITRATE: f64 = 300_000.0;
const MIN_BITRATE: f64 = 100_000.0;
const MAX_BITRATE: f64 = 5_000_000.0;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "bandwidth-estimation-from-disk")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "An example of bandwidth estimation driving quality selection.")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    /// Directory holding `low.ivf`, `med.ivf` and `high.ivf`.
    #[arg(short, long, default_value_t = format!("."))]
    video_dir: String,
}

// ── Estimator ─────────────────────────────────────────────────────────────────

/// Delegates to `inner` and publishes its target bitrate after every update.
///
/// The estimator is where an application belongs if it wants to watch the estimate: it is the one
/// object in the congestion control loop that is application-supplied, so wrapping it costs nothing
/// and reaches into no internals. Every method forwards; the only addition is the store after each
/// call that can move the number.
///
/// `target_bitrate` takes `&self`, so publishing cannot happen there. It happens after
/// [`on_reports`](BandwidthEstimator::on_reports) and
/// [`handle_timeout`](BandwidthEstimator::handle_timeout), which the interceptor's own contract
/// names as the two points where the estimate can change.
struct ReportingEstimator<E: BandwidthEstimator> {
    inner: E,
    target: Arc<AtomicU64>,
}

impl<E: BandwidthEstimator> ReportingEstimator<E> {
    fn new(inner: E) -> (Self, Arc<AtomicU64>) {
        let target = Arc::new(AtomicU64::new(inner.target_bitrate().to_bits()));
        let handle = Arc::clone(&target);
        (Self { inner, target }, handle)
    }

    fn publish(&self) {
        self.target
            .store(self.inner.target_bitrate().to_bits(), Ordering::Relaxed);
    }
}

impl<E: BandwidthEstimator> BandwidthEstimator for ReportingEstimator<E> {
    fn on_reports(&mut self, now: Instant, reports: &[PacketReport]) {
        self.inner.on_reports(now, reports);
        self.publish();
    }

    fn target_bitrate(&self) -> f64 {
        self.inner.target_bitrate()
    }

    fn handle_timeout(&mut self, now: Instant) {
        self.inner.handle_timeout(now);
        self.publish();
    }

    fn poll_timeout(&self) -> Option<Instant> {
        self.inner.poll_timeout()
    }

    fn stats(&self) -> EstimatorStats {
        self.inner.stats()
    }
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
    let video_dir = cli.video_dir.clone();

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

    // All three renditions have to exist before the call starts: discovering a missing file at the
    // moment the estimate says to switch would fail mid-stream, long after the mistake was made.
    for (file_name, _) in QUALITY_LEVELS {
        let path = Path::new(&video_dir).join(file_name);
        if !path.exists() {
            return Err(anyhow::anyhow!("video file '{}' not found", path.display()));
        }
    }

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

    // Create a MediaEngine object to configure the supported codec
    let mut media_engine = MediaEngine::default();

    let video_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 96,
    };
    media_engine.register_codec(video_codec.clone(), RtpCodecKind::Video)?;

    // Send-side congestion control, around Google Congestion Control. `configure_congestion_control`
    // places the send history, the pacer, the TWCC sender and the TWCC receiver at the slots the
    // chain reserves for them, and registers the `transport-cc` feedback and header extension the
    // remote needs in order to report arrivals at all — without which the estimator holds its
    // initial rate forever and never says anything is wrong.
    let (estimator, target_bitrate) =
        ReportingEstimator::new(Gcc::new(INITIAL_BITRATE, MIN_BITRATE, MAX_BITRATE));
    let registry = configure_congestion_control(
        Registry::new(),
        estimator,
        CongestionFeedback::Twcc,
        &mut media_engine,
    )?;
    let registry = register_default_interceptors(registry, &mut media_engine)?;

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

    let runtime = runtime();

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(Arc::new(Handler {
            gather_complete_tx,
            done_tx: done_tx.clone(),
            connected_tx,
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
        .build()
        .await?;

    let ssrc: SSRC = rand::random::<u32>();
    let video_track: Arc<TrackLocalStaticSample> = Arc::new(TrackLocalStaticSample::new(
        Instant::now(),
        MediaStreamTrack::new(
            "webrtc-rs-stream-id-video".to_owned(),
            "webrtc-rs-track-id-video".to_owned(),
            "webrtc-rs-track-label-video".to_owned(),
            RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(ssrc),
                    ..Default::default()
                },
                codec: video_codec.rtp_codec.clone(),
                ..Default::default()
            }],
        ),
    )?);
    let sender = peer_connection
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

    println!(
        "starting at {} kbps; switching between {}",
        (INITIAL_BITRATE / 1000.0) as u64,
        QUALITY_LEVELS
            .iter()
            .map(|(file_name, _)| *file_name)
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("Waiting for peer connection...");
    connected_rx.recv().await;
    println!("Connected! Starting media stream.");

    let payload_type = negotiated_payload_type(&sender).await?;
    let (video_done_tx, mut video_done_rx) = channel::<()>(1);
    runtime.spawn(Box::pin(async move {
        if let Err(err) = stream_video(video_dir, video_track, payload_type, target_bitrate).await {
            eprintln!("video streaming error: {err}");
        }
        let _ = video_done_tx.try_send(());
    }));

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => println!("received done signal!"),
        _ = ctrlc_rx.recv().fuse() => println!("received ctrl-c signal!"),
        _ = video_done_rx.recv().fuse() => println!("video streaming stopped"),
    }

    peer_connection.close().await?;

    Ok(())
}

// ── Streaming ─────────────────────────────────────────────────────────────────

async fn negotiated_payload_type(sender: &Arc<dyn RtpSender>) -> Result<PayloadType> {
    sender
        .get_parameters()
        .await?
        .rtp_parameters
        .codecs
        .first()
        .map(|codec| codec.payload_type)
        .ok_or_else(|| anyhow::anyhow!("sender has no negotiated codec"))
}

/// Opens one rendition, positioned past the IVF file header so `reset_reader` can use it directly.
fn open_rendition(video_dir: &str, file_name: &str) -> Result<BufReader<File>> {
    let mut file = File::open(Path::new(video_dir).join(file_name))?;
    file.seek(SeekFrom::Start(IVF_HEADER_SIZE))?;
    Ok(BufReader::new(file))
}

/// A reset closure for `IVFReader::reset_reader`, opening `file_name` past its header.
fn reset_to(video_dir: &str, file_name: &str) -> rtc::media::io::ResetFn<BufReader<File>> {
    let video_dir = video_dir.to_owned();
    let file_name = file_name.to_owned();
    Box::new(move |_bytes_read| open_rendition(&video_dir, &file_name).expect("reopen rendition"))
}

/// Whether a VP8 payload is a keyframe.
///
/// Bit 0 of the first byte of the uncompressed data chunk is the frame type (RFC 6386 §9.1): 0 for
/// a key frame, 1 for an interframe. This is what makes a mid-stream switch decodable — an
/// interframe from a file the receiver has not been watching references a reference frame it does
/// not have, and produces a smear until the next keyframe arrives.
fn is_keyframe(frame: &[u8]) -> bool {
    frame.first().is_some_and(|byte| byte & 0x1 == 0)
}

/// Switches to `new_quality` and returns the first frame that can be decoded from it.
///
/// Two conditions have to hold for that frame, and dropping either one produces a switch that
/// looks like it worked. It must be a **keyframe**, or the receiver decodes it against reference
/// frames from a file it was never sent. And its timestamp must be at or after the last one sent,
/// or the stream jumps backwards in time and the receiver discards everything until it catches up.
fn switch_quality_level(
    video_dir: &str,
    ivf: &mut IVFReader<BufReader<File>>,
    current_quality: usize,
    new_quality: usize,
    current_timestamp: u64,
) -> Option<(bytes::BytesMut, IVFFrameHeader)> {
    println!(
        "Switching from {} to {}",
        QUALITY_LEVELS[current_quality].0, QUALITY_LEVELS[new_quality].0
    );

    ivf.reset_reader(reset_to(video_dir, QUALITY_LEVELS[new_quality].0));

    loop {
        let (frame, frame_header) = ivf.parse_next_frame().ok()?;
        if frame_header.timestamp >= current_timestamp && is_keyframe(&frame) {
            return Some((frame, frame_header));
        }
    }
}

async fn stream_video(
    video_dir: String,
    video_track: Arc<TrackLocalStaticSample>,
    payload_type: PayloadType,
    target_bitrate: Arc<AtomicU64>,
) -> Result<()> {
    let mut current_quality = 0usize;
    println!("starting with {}", QUALITY_LEVELS[current_quality].0);

    let ssrc = *video_track
        .ssrcs()
        .await
        .first()
        .ok_or(Error::ErrSenderWithNoSSRCs)?;

    // The header comes from the first rendition; all three are encoded from the same source, so
    // they share a timebase.
    let first = File::open(Path::new(&video_dir).join(QUALITY_LEVELS[current_quality].0))?;
    let (mut ivf, header) = IVFReader::new(BufReader::new(first))?;

    // Pace at playback speed. Sending the file as fast as it parses would saturate the path and
    // make the estimator measure this example's own impatience rather than the network.
    let frame_duration = Duration::from_millis(
        ((1000 * header.timebase_numerator) / header.timebase_denominator) as u64,
    );
    let mut ticker = interval(frame_duration);

    let mut current_timestamp = 0u64;

    loop {
        ticker.tick().await;

        let target = f64::from_bits(target_bitrate.load(Ordering::Relaxed));

        // Two comparisons, and note they use different levels. Dropping down is judged against the
        // rendition being sent — if the path will not carry what is already going out, that is a
        // problem now. Climbing is judged against the *next* rendition, because there is no point
        // moving up until the estimate covers what the move would cost.
        let new_quality = if current_quality != 0 && target < QUALITY_LEVELS[current_quality].1 {
            Some(current_quality - 1)
        } else if current_quality + 1 < QUALITY_LEVELS.len()
            && target > QUALITY_LEVELS[current_quality + 1].1
        {
            Some(current_quality + 1)
        } else {
            None
        };

        let frame = match new_quality {
            Some(new_quality) => {
                let frame = switch_quality_level(
                    &video_dir,
                    &mut ivf,
                    current_quality,
                    new_quality,
                    current_timestamp,
                );
                // Committed even if the scan above found nothing usable: the reader is already on
                // the new file, so leaving `current_quality` behind would make the next reset
                // reopen a rendition that is not the one being read.
                current_quality = new_quality;
                frame
            }
            None => ivf.parse_next_frame().ok(),
        };

        let Some((frame, frame_header)) = frame else {
            // End of file — loop the rendition rather than stopping. The example is about the
            // estimate, and it needs a stream that outlives the file.
            ivf.reset_reader(reset_to(&video_dir, QUALITY_LEVELS[current_quality].0));
            current_timestamp = 0;
            continue;
        };

        current_timestamp = frame_header.timestamp;

        video_track
            .sample_writer(ssrc, payload_type)
            .write_sample(&Sample {
                data: frame.freeze(),
                duration: frame_duration,
                ..Sample::new(Instant::now())
            })
            .await?;
    }
}
