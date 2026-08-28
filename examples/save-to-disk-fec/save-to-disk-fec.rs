//! save-to-disk-fec: receive a FlexFEC-03 protected stream, rebuild what the path lost, save it.
//!
//! The receiving half of [`play-from-disk-fec`](../play-from-disk-fec). That example protects a VP8
//! stream with FlexFEC-03 and then deliberately drops media packets at the wire; this one accepts
//! the offer, rebuilds the dropped packets from the repair stream, and writes the result to an IVF
//! file.
//!
//! There is no pion counterpart — pion has no FlexFEC *receiver*.
//!
//! # What makes it work
//!
//! Two things, and neither is much code:
//!
//! - **`video/flexfec-03` registered in the `MediaEngine`.** As the answerer this endpoint can only
//!   select payload types the offer listed, so the repair codec has to be here for the offer's
//!   `a=rtpmap:49 flexfec-03/90000` to be answered. Leave it out and everything still runs: the
//!   connection comes up, the file fills, and the sender's induced loss goes straight to disk.
//! - **`FlexFec03Receive` at [`Slot::FecDecoder`]**, wire-ward of everything that inspects sequence
//!   numbers. A rebuilt packet has to be indistinguishable from one that arrived, so it must rejoin
//!   the stream before the NACK generator — which would otherwise ask the sender for a packet
//!   already being rebuilt here — and before the jitter buffer, which has to order it with the rest.
//!
//! [`RecoveryCounter`] adds nothing to that; it only reports, so the recovery is visible.

use anyhow::Result;
use clap::Parser;
use env_logger::Target;
use futures::FutureExt;
use rtc::interceptor::{
    Attribute, FlexFec03ReceiveBuilder, Interceptor, Packet, Registry, Slot, StreamInfo,
    TaggedPacket,
};
use rtc::media::io::Writer;
use rtc::media::io::ivf_reader::IVFFileHeader;
use rtc::media::io::ivf_writer::IVFWriter;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{
    MIME_TYPE_FLEX_FEC03, MIME_TYPE_VP8, MediaEngine,
};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit, SSRC};
use rtc::sansio::Protocol;
use rtc::shared::error::Error as RtcError;
use std::collections::VecDeque;
use std::fs::File;
use std::sync::Arc;
use std::time::Instant;
use std::{fs, fs::OpenOptions, io::Write as IoWrite, str::FromStr};
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel};

#[path = "../common/mod.rs"]
mod common;
use common::{block_on, runtime};

/// The FlexFEC-03 payload type, matching what `play-from-disk-fec` offers.
const FLEX_FEC_PAYLOAD_TYPE: u8 = 49;

/// Where [`RecoveryCounter`] sits: immediately application-ward of `Slot::FecDecoder` (6_000).
///
/// The read walk runs from the wire up to the application, so this is the first thing a packet
/// meets after the decoder has had its say — the earliest point at which
/// [`Attribute::RecoveredByFec`] exists to be counted. Anywhere wire-ward of the decoder it would
/// count nothing and report a recovery rate of zero on a connection recovering perfectly well.
const RECOVERY_COUNTER_SLOT: usize = 6_500;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "save-to-disk-fec")]
#[command(author = "Rain Liu <yliu@webrtc.rs>")]
#[command(version = "0.1.0")]
#[command(about = "Receive a FlexFEC-03 protected stream and save the recovered video.")]
struct Cli {
    #[arg(short, long)]
    debug: bool,
    #[arg(short, long, default_value_t = format!("INFO"))]
    log_level: String,
    #[arg(short, long, default_value_t = format!(""))]
    input_sdp_file: String,
    #[arg(short, long, default_value_t = format!(""))]
    output_log_file: String,
    #[arg(short, long, default_value = "output.ivf")]
    video: String,
}

// ── Recovery counter ──────────────────────────────────────────────────────────

/// Counts inbound media, separating what arrived from what the FEC decoder rebuilt.
///
/// Purely an observer: every packet is passed through untouched. It exists because recovery is
/// invisible from the outside — a rebuilt packet is deliberately indistinguishable from one that
/// arrived, which is what makes FEC work and also what makes it impossible to tell whether it is
/// working at all. The attribute is the one place that distinction survives, and it survives only
/// inside the chain: a `TrackRemoteEvent::OnRtpPacket` carries the packet and nothing else.
struct RecoveryCounter {
    /// Repair SSRCs, so repair traffic is not counted as media.
    fec_ssrcs: Vec<SSRC>,
    arrived: u64,
    recovered: u64,
    /// Repair packets that reached this far. Expected to stay at zero — see [`Self::report`].
    fec_packets: u64,
    read_queue: VecDeque<TaggedPacket>,
    write_queue: VecDeque<TaggedPacket>,
}

impl RecoveryCounter {
    fn new() -> Self {
        Self {
            fec_ssrcs: Vec::new(),
            arrived: 0,
            recovered: 0,
            fec_packets: 0,
            read_queue: VecDeque::new(),
            write_queue: VecDeque::new(),
        }
    }

    /// `Unrouted FEC` is expected to read 0: the decoder is wire-ward of here and consumes repair
    /// packets it has a decoder for. A non-zero value means repair packets arrived for a stream
    /// that never bound — negotiated but unusable — which otherwise looks identical to a path that
    /// simply lost nothing.
    fn report(&self) {
        let media = self.arrived + self.recovered;
        let rate = if media == 0 {
            0.0
        } else {
            self.recovered as f64 / media as f64
        };
        println!(
            "Stats: Media: {media} (arrived: {}, recovered: {}), Unrouted FEC: {}, Recovered: {:.4}%",
            self.arrived,
            self.recovered,
            self.fec_packets,
            rate * 100.0
        );
    }
}

impl Protocol<TaggedPacket, TaggedPacket, ()> for RecoveryCounter {
    type Rout = TaggedPacket;
    type Wout = TaggedPacket;
    type Eout = ();
    type Error = RtcError;
    type Time = Instant;

    fn handle_read(&mut self, msg: TaggedPacket) -> Result<(), Self::Error> {
        if let Packet::Rtp(ref rtp_packet) = msg.message.packet {
            if self.fec_ssrcs.contains(&rtp_packet.header.ssrc) {
                self.fec_packets += 1;
            } else {
                if msg.message.has(&Attribute::RecoveredByFec) {
                    self.recovered += 1;
                } else {
                    self.arrived += 1;
                }

                if (self.arrived + self.recovered).is_multiple_of(100) {
                    self.report();
                }
            }
        }

        self.read_queue.push_back(msg);
        Ok(())
    }

    fn poll_read(&mut self) -> Option<Self::Rout> {
        self.read_queue.pop_front()
    }

    fn handle_write(&mut self, msg: TaggedPacket) -> Result<(), Self::Error> {
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

impl Interceptor for RecoveryCounter {
    fn bind_remote_stream(&mut self, info: &StreamInfo) {
        // The repair flow is bound in its own right, as a real RTP stream with its own SSRC and
        // sequence-number space — so this runs twice per protected stream. The repair flow's own
        // bind carries no `ssrc_fec` (it repairs nothing), and must not be mistaken for a media
        // stream that failed to negotiate FEC.
        if info.payload_type == FLEX_FEC_PAYLOAD_TYPE {
            return;
        }

        if let Some(ssrc_fec) = info.ssrc_fec {
            println!(
                "FEC negotiated: media SSRC {} protected by repair SSRC {ssrc_fec}",
                info.ssrc
            );
            self.fec_ssrcs.push(ssrc_fec);
        } else {
            // Worth saying out loud. Everything still runs, the file still fills up, and the loss
            // the sender induced goes straight to disk — a silent no-op is the failure mode this
            // example is most likely to hit.
            println!(
                "no FEC for media SSRC {} — nothing will be recovered",
                info.ssrc
            );
        }
    }

    fn unbind_remote_stream(&mut self, info: &StreamInfo) {
        if let Some(ssrc_fec) = info.ssrc_fec {
            self.fec_ssrcs.retain(|ssrc| *ssrc != ssrc_fec);
        }
    }

    fn bind_local_stream(&mut self, _info: &StreamInfo) {}
    fn unbind_local_stream(&mut self, _info: &StreamInfo) {}
}

// ── Event handler ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Handler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    done_tx: Sender<()>,
    video_writer: Arc<std::sync::Mutex<Option<IVFWriter<File>>>>,
    video_file: String,
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
                println!("Ctrl-C play-from-disk-fec, or this side, to stop the demo");
            }
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                let _ = self.done_tx.try_send(());
            }
            _ => {}
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let Some(&media_ssrc) = track.ssrcs().await.first() else {
            return;
        };
        let mime_type = track
            .codec(media_ssrc)
            .await
            .map(|codec| codec.mime_type)
            .unwrap_or_default();
        println!(
            "Got {mime_type} track, saving to disk as {}",
            self.video_file
        );

        // Recovered packets arrive here exactly like any other: by the time a packet reaches the
        // application the decoder has already put it back in the stream, so there is nothing to do
        // differently. That is the point.
        let video_writer = Arc::clone(&self.video_writer);
        self.runtime.spawn(Box::pin(async move {
            while let Some(TrackRemoteEvent::OnRtpPacket(packet)) = track.poll().await {
                let mut guard = video_writer.lock().unwrap();
                if let Some(ref mut writer) = *guard
                    && let Err(err) = writer.write_rtp(&packet)
                {
                    println!("video write_rtp error: {err}");
                    break;
                }
            }
            let mut guard = video_writer.lock().unwrap();
            if let Some(ref mut writer) = *guard
                && let Err(err) = writer.close()
            {
                println!("video file close error: {err}");
            }
            println!("Video track ended, file closed.");
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

    // Everything below is the WebRTC-rs API! Thanks for using it ❤️.

    let video_writer: Arc<std::sync::Mutex<Option<IVFWriter<File>>>> =
        Arc::new(std::sync::Mutex::new(Some(IVFWriter::new(
            File::create(&cli.video)?,
            &IVFFileHeader {
                signature: *b"DKIF",
                version: 0,
                header_size: 32,
                four_cc: *b"VP80",
                width: 640,
                height: 480,
                timebase_denominator: 30,
                timebase_numerator: 1,
                num_frames: 900,
                unused: 0,
            },
        )?)));

    // Create a MediaEngine object to configure the supported codec
    let mut media_engine = MediaEngine::default();

    media_engine.register_codec(
        RTCRtpCodecParameters {
            rtp_codec: RTCRtpCodec {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 96,
        },
        RtpCodecKind::Video,
    )?;

    // The repair codec. An answerer selects from what the offer listed, so this has to be here for
    // the offer's `a=rtpmap:49 flexfec-03/90000` to be answered — and if it is not, everything
    // still runs and nothing is ever recovered.
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

    // The interceptor chain. Slots decide the order, not the sequence of these calls. On the read
    // walk, from the wire up to the application:
    //
    //   6_000  FlexFec03Receive — rebuilds media from the repair stream
    //   6_500  RecoveryCounter  — reports what was rebuilt
    //   …             everything `register_default_interceptors` adds
    let registry = Registry::new()
        .with(Slot::FecDecoder, FlexFec03ReceiveBuilder::new().build())
        .with(Slot::from(RECOVERY_COUNTER_SLOT), RecoveryCounter::new());
    let registry = register_default_interceptors(registry, &mut media_engine)?;

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

    let runtime = runtime();

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(Arc::new(Handler {
            runtime: runtime.clone(),
            gather_complete_tx,
            done_tx: done_tx.clone(),
            video_writer,
            video_file: cli.video.clone(),
        }))
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec![format!("{}:0", signal::get_local_ip())])
        .build()
        .await?;

    // Allow us to receive 1 video track
    peer_connection
        .add_transceiver_from_kind(
            RtpCodecKind::Video,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                ..Default::default()
            }),
        )
        .await?;

    // Wait for the offer to be pasted
    print!("Paste offer from play-from-disk-fec and press Enter: ");

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

    println!("Press ctrl-c to stop");
    futures::select! {
        _ = done_rx.recv().fuse() => println!("received done signal!"),
        _ = ctrlc_rx.recv().fuse() => println!("received ctrl-c signal!"),
    }

    peer_connection.close().await?;
    println!("Done writing {}", cli.video);

    Ok(())
}
