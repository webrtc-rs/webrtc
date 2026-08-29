//! play-from-disk-fec: send video with FlexFEC-03 repair, over a path that drops packets on purpose.
//!
//! A port of pion's `play-from-disk-fec`. The point is not that video plays — that is
//! `play-from-disk-vpx` — but that it *keeps* playing while a fraction of the media packets are
//! discarded on the way out. A [`DropFilter`] interceptor stands at the wire end of the chain and
//! throws media away after everything else has run, which is what makes the recovery visible: the
//! browser receives a stream with holes in it and reconstructs the missing packets from the repair
//! stream.
//!
//! Pair it with [`save-to-disk-fec`](../save-to-disk-fec) to watch the other side rebuild.
//!
//! # This example offers; it does not answer
//!
//! Unlike most examples here, and not for style: Chrome accepts `video/flexfec-03` when it is
//! offered to it, but does not put it in its own offers. As the answerer we could only choose among
//! the payload types the browser listed, so there would be no FEC to select, the encoder would
//! never bind, and the example would run to completion showing dropped video and no recovery.
//!
//! # Where the drop sits
//!
//! At [`DROP_FILTER_SLOT`], below every built-in interceptor, because it is standing in for the
//! network. Anywhere higher and some sender-side mechanism would be told about a loss the network
//! caused: above the FEC encoder, nothing would ever be protected and the example would recover
//! nothing; above the NACK responder, dropped packets would never enter the retransmission buffer;
//! above congestion control, the estimator would not count the bytes that were sent.

use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::{
    FlexFec03SendBuilder, Interceptor, Packet, Registry, Slot, StreamInfo, TaggedPacket,
};
use rtc::media::Sample;
use rtc::media::io::ivf_reader::IVFReader;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{
    MIME_TYPE_FLEX_FEC03, MIME_TYPE_VP8, MediaEngine,
};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RTCRtpFecParameters, RtpCodecKind,
};
use rtc::rtp_transceiver::{PayloadType, SSRC};
use rtc::sansio::Protocol;
use rtc::shared::error::Error as RtcError;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
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

/// The FlexFEC-03 payload type, matching pion's `ConfigureFlexFEC03(49, …)`.
const FLEX_FEC_PAYLOAD_TYPE: u8 = 49;

/// One repair packet per ten media packets: it recovers a single loss anywhere in the block.
///
/// The example drops more than that by default, so the browser will still see gaps. That is the
/// honest picture: FEC narrows the loss it is sized for and no more, and a block that loses two of
/// ten is not recoverable however the repair is arranged.
const NUM_MEDIA_PACKETS: u32 = 10;
const NUM_FEC_PACKETS: u32 = 1;

/// Where [`DropFilter`] sits: below `Slot::CongestionControl` (1_000), the lowest built-in slot.
///
/// The write walk runs from the application down to the wire, so this is the last thing a
/// departing packet meets — exactly where a network loss belongs. It matches where pion's
/// `packetDropInterceptorFactory` ends up, since pion's chain puts the first-registered
/// interceptor closest to the wire.
const DROP_FILTER_SLOT: usize = 500;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "play-from-disk-fec")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "An example of play-from-disk with FlexFEC-03 over a lossy path.")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    /// Video file to stream (.ivf containing VP8)
    #[arg(short, long)]
    video: String,
    /// Drop one media packet in this many. `0` disables dropping, which is the way to see what the
    /// stream looks like without the loss this example exists to survive.
    #[arg(long, default_value_t = 5)]
    drop_one_in: u32,
}

// ── Drop filter ───────────────────────────────────────────────────────────────

/// Discards outgoing media packets to simulate a lossy path, and counts what it did.
///
/// Repair packets are never dropped: they are identified by the FEC SSRC the stream negotiated and
/// pass through untouched. Dropping them too would only add a second, uninteresting variable.
struct DropFilter {
    /// Drop one media packet in this many; `0` disables.
    drop_one_in: u32,
    /// The repair SSRC for each protected stream, so repair traffic is exempt.
    fec_ssrcs: Vec<SSRC>,
    media_packets: u64,
    fec_packets: u64,
    dropped_packets: u64,
    read_queue: VecDeque<TaggedPacket>,
    write_queue: VecDeque<TaggedPacket>,
}

impl DropFilter {
    fn new(drop_one_in: u32) -> Self {
        Self {
            drop_one_in,
            fec_ssrcs: Vec::new(),
            media_packets: 0,
            fec_packets: 0,
            dropped_packets: 0,
            read_queue: VecDeque::new(),
            write_queue: VecDeque::new(),
        }
    }

    fn report(&self) {
        let ratio = if self.media_packets == 0 {
            0.0
        } else {
            self.dropped_packets as f64 / self.media_packets as f64
        };
        println!(
            "Stats: Media: {}, FEC: {}, Dropped: {}, Drop ratio: {:.4}%",
            self.media_packets,
            self.fec_packets,
            self.dropped_packets,
            ratio * 100.0
        );
    }
}

impl Protocol<TaggedPacket, TaggedPacket, ()> for DropFilter {
    type Rout = TaggedPacket;
    type Wout = TaggedPacket;
    type Eout = ();
    type Error = RtcError;
    type Time = Instant;

    fn handle_read(&mut self, msg: TaggedPacket) -> Result<(), Self::Error> {
        self.read_queue.push_back(msg);
        Ok(())
    }

    fn poll_read(&mut self) -> Option<Self::Rout> {
        self.read_queue.pop_front()
    }

    fn handle_write(&mut self, msg: TaggedPacket) -> Result<(), Self::Error> {
        let Packet::Rtp(ref rtp_packet) = msg.message.packet else {
            // RTCP is control traffic; dropping it would break feedback rather than demonstrate
            // recovery.
            self.write_queue.push_back(msg);
            return Ok(());
        };

        if self.fec_ssrcs.contains(&rtp_packet.header.ssrc) {
            self.fec_packets += 1;
            self.write_queue.push_back(msg);
            return Ok(());
        }

        if self.media_packets.is_multiple_of(100) {
            self.report();
        }
        self.media_packets += 1;

        if self.drop_one_in != 0 && self.media_packets.is_multiple_of(self.drop_one_in as u64) {
            self.dropped_packets += 1;
            // Swallowed: the packet is not queued, so nothing below this interceptor ever sees it,
            // exactly as if the path had lost it.
            return Ok(());
        }

        self.write_queue.push_back(msg);
        Ok(())
    }

    fn poll_write(&mut self) -> Option<Self::Wout> {
        self.write_queue.pop_front()
    }

    fn handle_timeout(&mut self, _now: Instant) -> Result<(), Self::Error> {
        Ok(())
    }

    fn poll_timeout(&mut self) -> Option<Self::Time> {
        None
    }
}

impl Interceptor for DropFilter {
    fn bind_local_stream(&mut self, info: &StreamInfo) {
        // Learn the repair SSRC from the stream that negotiated FEC, so `handle_write` can tell
        // repair from media without guessing.
        if let Some(ssrc_fec) = info.ssrc_fec {
            self.fec_ssrcs.push(ssrc_fec);
        }
    }

    fn unbind_local_stream(&mut self, info: &StreamInfo) {
        if let Some(ssrc_fec) = info.ssrc_fec {
            self.fec_ssrcs.retain(|ssrc| *ssrc != ssrc_fec);
        }
    }

    fn bind_remote_stream(&mut self, _info: &StreamInfo) {}
    fn unbind_remote_stream(&mut self, _info: &StreamInfo) {}
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
    let video_file = cli.video.clone();
    let drop_one_in = cli.drop_one_in;

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

    if !Path::new(&video_file).exists() {
        return Err(anyhow::anyhow!("video file: '{}' not exist", video_file));
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

    // The repair stream is a codec in its own right, and both halves of the association have to be
    // negotiated: a FEC SSRC with no payload type is not a usable repair flow, and the encoder
    // declines to bind without both.
    media_engine.register_codec(
        RTCRtpCodecParameters {
            rtp_codec: RTCRtpCodec {
                mime_type: MIME_TYPE_FLEX_FEC03.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "repair-window=10000000".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: FLEX_FEC_PAYLOAD_TYPE,
        },
        RtpCodecKind::Video,
    )?;

    // The interceptor chain. Slots decide the order, not the sequence of these calls. On the write
    // walk, from the application down to the wire:
    //
    //   5_000  FlexFec03Send  — builds the repair block from the complete media stream
    //   …             everything `register_default_interceptors` adds
    //     500  DropFilter     — the "network", discarding media last of all
    let registry = Registry::new()
        .with(
            Slot::FecEncoder,
            FlexFec03SendBuilder::new()
                .with_num_media_packets(NUM_MEDIA_PACKETS)
                .with_num_fec_packets(NUM_FEC_PACKETS)
                .build(),
        )
        .with(Slot::from(DROP_FILTER_SLOT), DropFilter::new(drop_one_in));
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
    let fec_ssrc: SSRC = rand::random::<u32>();

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
                    // The other half of the repair association: the SSRC the encoder sends repair
                    // packets on, and the one `DropFilter` exempts. Naming it here rather than
                    // letting one be minted keeps every SSRC this example puts on the wire explicit.
                    fec: Some(RTCRtpFecParameters { ssrc: fec_ssrc }),
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

    // Create an offer. See the module docs: this side must offer, or FlexFEC is never negotiated.
    let offer = peer_connection.create_offer(None).await?;
    peer_connection.set_local_description(offer).await?;

    // Block until ICE Gathering is complete (non-trickle ICE)
    let _ = gather_complete_rx.recv().await;

    if let Some(local_desc) = peer_connection.local_description().await {
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = signal::encode(&json_str);
        println!("{b64}");
    } else {
        println!("generate local_description failed!");
    }

    // Wait for the answer to be pasted
    println!("Paste answer from browser and press Enter:");
    let line = if cli.input_sdp_file.is_empty() {
        signal::must_read_stdin()?
    } else {
        // The file is read on Enter, not at startup: the browser has not produced the answer yet
        // when this process begins, so there would be nothing there to read.
        println!("(reading it from {})", cli.input_sdp_file);
        signal::must_read_stdin()?;
        fs::read_to_string(&cli.input_sdp_file)?
    };
    let desc_data = signal::decode(line.as_str())?;
    let answer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;
    println!("Answer received: {answer}");
    peer_connection.set_remote_description(answer).await?;

    if drop_one_in == 0 {
        println!("dropping disabled: this is play-from-disk with FEC and no induced loss");
    } else {
        println!(
            "dropping 1 media packet in {drop_one_in} at the wire; \
             repair is {NUM_FEC_PACKETS} per {NUM_MEDIA_PACKETS}"
        );
    }

    println!("Waiting for peer connection...");
    connected_rx.recv().await;
    println!("Connected! Starting media stream.");

    let payload_type = media_payload_type(&sender).await?;
    let (video_done_tx, mut video_done_rx) = channel::<()>(1);
    runtime.spawn(Box::pin(async move {
        if let Err(err) = stream_video(video_file, video_track, payload_type).await {
            eprintln!("video streaming error: {err}");
        }
        let _ = video_done_tx.try_send(());
    }));

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => println!("received done signal!"),
        _ = ctrlc_rx.recv().fuse() => println!("received ctrl-c signal!"),
        _ = video_done_rx.recv().fuse() => println!("All video frames parsed and sent"),
    }

    peer_connection.close().await?;

    Ok(())
}

// ── Streaming ─────────────────────────────────────────────────────────────────

/// The negotiated payload type of the *media* codec.
///
/// Not `codecs.first()`, as the other examples can afford: this sender's codec list also contains
/// the repair codec, and stamping frames with that payload type would send the video as if it were
/// FEC.
async fn media_payload_type(sender: &Arc<dyn RtpSender>) -> Result<PayloadType> {
    sender
        .get_parameters()
        .await?
        .rtp_parameters
        .codecs
        .iter()
        .find(|codec| !codec.rtp_codec.mime_type.to_lowercase().contains("flexfec"))
        .map(|codec| codec.payload_type)
        .ok_or_else(|| anyhow::anyhow!("sender has no negotiated media codec"))
}

async fn stream_video(
    video_file_name: String,
    video_track: Arc<TrackLocalStaticSample>,
    payload_type: PayloadType,
) -> Result<()> {
    let file = File::open(&video_file_name)?;
    let reader = BufReader::new(file);
    let (mut ivf, header) = IVFReader::new(reader)?;

    println!("play video from disk file {video_file_name}");
    let ssrc = *video_track
        .ssrcs()
        .await
        .first()
        .ok_or(Error::ErrSenderWithNoSSRCs)?;

    // Send the file a frame at a time, paced at playback speed. Sending it all at once would
    // produce loss of its own and confuse the loss this example induces on purpose.
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
            .sample_writer(ssrc, payload_type)
            .write_sample(&Sample {
                data: frame.freeze(),
                duration: Duration::from_secs(1),
                ..Sample::new(Instant::now())
            })
            .await?;

        let _ = ticker.tick().await;
    }

    Ok(())
}
