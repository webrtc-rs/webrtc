//! FlexFEC-03 end to end: the encoder protects, the path loses, the decoder rebuilds.
//!
//! The two halves of [`play-from-disk-fec`](../examples/play-from-disk-fec) and
//! [`save-to-disk-fec`](../examples/save-to-disk-fec) in one process, with the assertion those
//! examples can only gesture at: **every media packet the sender discarded arrives at the
//! receiver, and arrives marked as rebuilt.**
//!
//! # Why sequence numbers rather than counts
//!
//! A recovered packet is re-injected where the decoder finishes rebuilding it, not where it would
//! have arrived, so the inbound stream is not in sequence order. Counting is therefore not enough —
//! "we dropped 10 and received 200" holds just as well if FEC rebuilt the wrong ten. Both sides
//! report the sequence numbers they acted on and the test compares the two *sets*, which is order
//! independent and says exactly which packets were involved.
//!
//! # Why the chain is this small
//!
//! Neither side calls `register_default_interceptors`. With the NACK pair in the chain the receiver
//! would ask for the missing packets and the sender would retransmit them, so the packets would
//! come back and the test would pass with the FEC decoder doing nothing at all. Retransmission is a
//! perfectly good recovery mechanism; it is just not the one under test.

use rtc::interceptor::{
    Attribute, FlexFec03ReceiveBuilder, FlexFec03SendBuilder, Interceptor, Packet, Registry, Slot,
    StreamInfo, TaggedPacket,
};
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::media_engine::{
    MIME_TYPE_FLEX_FEC03, MIME_TYPE_VP8, MediaEngine,
};
use rtc::rtp;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RTCRtpFecParameters, RtpCodecKind,
};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit, SSRC};
use rtc::sansio::Protocol;
use rtc::shared::error::Error as RtcError;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel as std_channel};
use std::time::{Duration, Instant};
use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::channel;

mod common;
use common::{block_on, interval, runtime, timeout};

const VP8_PT: u8 = 96;
const FLEX_FEC_PT: u8 = 49;

const MEDIA_SSRC: SSRC = 0x1111_1111;
const FEC_SSRC: SSRC = 0x2222_2222;

/// One repair packet per ten media packets recovers a single loss anywhere in the block.
const NUM_MEDIA_PACKETS: u32 = 10;
const NUM_FEC_PACKETS: u32 = 1;

/// Drop one media packet in twenty.
///
/// Chosen against the repair rate above, not for realism: at most one loss lands in any block of
/// ten, so every loss is recoverable and the test can demand *all* of them back. At one in five —
/// what `play-from-disk-fec` defaults to — most blocks lose two and the correct expectation would
/// be a fraction, which is a much weaker thing to assert.
const DROP_ONE_IN: u64 = 20;

/// Enough to exercise twenty FEC blocks, so a systematic failure cannot hide behind one lucky one.
const MEDIA_PACKETS_TO_SEND: u16 = 200;

/// The drop filter stands in for the network: below every built-in slot, so the packet is gone only
/// after everything on the sending side has already accounted for it.
const DROP_FILTER_SLOT: usize = 500;

/// The recorder sits immediately application-ward of `Slot::FecDecoder` (6_000) — the earliest
/// point at which [`Attribute::RecoveredByFec`] exists to be read.
const RECOVERY_RECORDER_SLOT: usize = 6_500;

// ── Interceptors ──────────────────────────────────────────────────────────────

/// Discards one media packet in [`DROP_ONE_IN`] and reports the sequence number it discarded.
///
/// Repair packets are exempt: dropping those would be testing something else. They are identified
/// by the FEC SSRC the stream negotiated, learned at bind.
///
/// Reporting goes over a `std::sync::mpsc` rather than a runtime channel: this test runs under
/// whichever runtime the build selected, and an interceptor's `handle_write` is synchronous, so the
/// send must not need an executor.
struct DropFilter {
    fec_ssrcs: Vec<SSRC>,
    media_packets: u64,
    dropped_tx: Sender<u16>,
    read_queue: VecDeque<TaggedPacket>,
    write_queue: VecDeque<TaggedPacket>,
}

impl DropFilter {
    fn new(dropped_tx: Sender<u16>) -> Self {
        Self {
            fec_ssrcs: Vec::new(),
            media_packets: 0,
            dropped_tx,
            read_queue: VecDeque::new(),
            write_queue: VecDeque::new(),
        }
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
            self.write_queue.push_back(msg);
            return Ok(());
        };

        if self.fec_ssrcs.contains(&rtp_packet.header.ssrc) {
            self.write_queue.push_back(msg);
            return Ok(());
        }

        self.media_packets += 1;
        if self.media_packets.is_multiple_of(DROP_ONE_IN) {
            let _ = self.dropped_tx.send(rtp_packet.header.sequence_number);
            // Not queued: nothing below this point ever sees it, exactly as if the path had lost it.
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

/// Reports every inbound media packet's sequence number, and whether the decoder rebuilt it.
struct RecoveryRecorder {
    fec_ssrcs: Vec<SSRC>,
    arrivals_tx: Sender<(u16, bool)>,
    read_queue: VecDeque<TaggedPacket>,
    write_queue: VecDeque<TaggedPacket>,
}

impl RecoveryRecorder {
    fn new(arrivals_tx: Sender<(u16, bool)>) -> Self {
        Self {
            fec_ssrcs: Vec::new(),
            arrivals_tx,
            read_queue: VecDeque::new(),
            write_queue: VecDeque::new(),
        }
    }
}

impl Protocol<TaggedPacket, TaggedPacket, ()> for RecoveryRecorder {
    type Rout = TaggedPacket;
    type Wout = TaggedPacket;
    type Eout = ();
    type Error = RtcError;
    type Time = Instant;

    fn handle_read(&mut self, msg: TaggedPacket) -> Result<(), Self::Error> {
        if let Packet::Rtp(ref rtp_packet) = msg.message.packet
            && !self.fec_ssrcs.contains(&rtp_packet.header.ssrc)
        {
            let _ = self.arrivals_tx.send((
                rtp_packet.header.sequence_number,
                msg.message.has(&Attribute::RecoveredByFec),
            ));
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

impl Interceptor for RecoveryRecorder {
    fn bind_remote_stream(&mut self, info: &StreamInfo) {
        if let Some(ssrc_fec) = info.ssrc_fec {
            self.fec_ssrcs.push(ssrc_fec);
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

// ── Handlers ──────────────────────────────────────────────────────────────────

struct Handler {
    gather_complete_tx: webrtc::runtime::Sender<()>,
    connected_tx: webrtc::runtime::Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if state == RTCPeerConnectionState::Connected {
            let _ = self.connected_tx.try_send(());
        }
    }
}

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn video_codec() -> RTCRtpCodecParameters {
    RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: String::new(),
            rtcp_feedback: vec![],
        },
        payload_type: VP8_PT,
    }
}

fn fec_codec() -> RTCRtpCodecParameters {
    RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_FLEX_FEC03.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "repair-window=10000000".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: FLEX_FEC_PT,
    }
}

fn media_engine() -> MediaEngine {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_codec(video_codec(), RtpCodecKind::Video)
        .expect("register vp8");
    media_engine
        .register_codec(fec_codec(), RtpCodecKind::Video)
        .expect("register flexfec-03");
    media_engine
}

// ── Test ──────────────────────────────────────────────────────────────────────

#[test]
fn flexfec03_recovers_every_dropped_packet() {
    block_on(async {
        let runtime = runtime();

        let (dropped_tx, dropped_rx) = std_channel::<u16>();
        let (arrivals_tx, arrivals_rx) = std_channel::<(u16, bool)>();

        // ---- sender (offerer) ----
        let mut offerer_media_engine = media_engine();
        let offerer_registry = Registry::new()
            .with(
                Slot::FecEncoder,
                FlexFec03SendBuilder::new()
                    .with_num_media_packets(NUM_MEDIA_PACKETS)
                    .with_num_fec_packets(NUM_FEC_PACKETS)
                    .build(),
            )
            .with(Slot::from(DROP_FILTER_SLOT), DropFilter::new(dropped_tx));

        let (off_gather_tx, mut off_gather_rx) = channel(1);
        let (off_conn_tx, mut off_conn_rx) = channel(1);
        let offerer = Arc::new(
            PeerConnectionBuilder::new()
                .with_media_engine(std::mem::take(&mut offerer_media_engine))
                .with_interceptor_registry(offerer_registry)
                .with_handler(Arc::new(Handler {
                    gather_complete_tx: off_gather_tx,
                    connected_tx: off_conn_tx,
                }))
                .with_runtime(runtime.clone())
                .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
                .build()
                .await
                .expect("build offerer"),
        );

        let video_track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
            "stream".to_owned(),
            "video".to_owned(),
            "video".to_owned(),
            RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(MEDIA_SSRC),
                    fec: Some(RTCRtpFecParameters { ssrc: FEC_SSRC }),
                    ..Default::default()
                },
                codec: video_codec().rtp_codec,
                ..Default::default()
            }],
        )));
        offerer
            .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal>)
            .await
            .expect("add track");

        // ---- receiver (answerer) ----
        let mut answerer_media_engine = media_engine();
        let answerer_registry = Registry::new()
            .with(Slot::FecDecoder, FlexFec03ReceiveBuilder::new().build())
            .with(
                Slot::from(RECOVERY_RECORDER_SLOT),
                RecoveryRecorder::new(arrivals_tx),
            );

        let (ans_gather_tx, mut ans_gather_rx) = channel(1);
        let (ans_conn_tx, mut ans_conn_rx) = channel(1);
        let answerer = Arc::new(
            PeerConnectionBuilder::new()
                .with_media_engine(std::mem::take(&mut answerer_media_engine))
                .with_interceptor_registry(answerer_registry)
                .with_handler(Arc::new(Handler {
                    gather_complete_tx: ans_gather_tx,
                    connected_tx: ans_conn_tx,
                }))
                .with_runtime(runtime.clone())
                .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
                .build()
                .await
                .expect("build answerer"),
        );
        answerer
            .add_transceiver_from_kind(
                RtpCodecKind::Video,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    ..Default::default()
                }),
            )
            .await
            .expect("add recvonly transceiver");

        // ---- negotiate ----
        let offer = offerer.create_offer(None).await.expect("create offer");
        offerer
            .set_local_description(offer)
            .await
            .expect("set local offer");
        timeout(Duration::from_secs(5), off_gather_rx.recv())
            .await
            .expect("offerer gathering");
        let offer_sdp = offerer.local_description().await.expect("local offer");

        assert!(
            offer_sdp
                .sdp
                .contains(&format!("a=rtpmap:{FLEX_FEC_PT} flexfec-03/90000")),
            "the offer must carry the repair codec, or nothing below tests FEC:\n{}",
            offer_sdp.sdp
        );
        assert!(
            offer_sdp
                .sdp
                .contains(&format!("a=ssrc-group:FEC-FR {MEDIA_SSRC} {FEC_SSRC}")),
            "the offer must group the repair flow with the media it repairs:\n{}",
            offer_sdp.sdp
        );

        answerer
            .set_remote_description(offer_sdp)
            .await
            .expect("answerer set remote");
        let answer = answerer.create_answer(None).await.expect("create answer");
        answerer
            .set_local_description(answer)
            .await
            .expect("set local answer");
        timeout(Duration::from_secs(5), ans_gather_rx.recv())
            .await
            .expect("answerer gathering");
        let answer_sdp = answerer.local_description().await.expect("local answer");

        assert!(
            answer_sdp
                .sdp
                .contains(&format!("a=rtpmap:{FLEX_FEC_PT} flexfec-03/90000")),
            "the answerer must select the repair codec, or its decoder never binds:\n{}",
            answer_sdp.sdp
        );

        offerer
            .set_remote_description(answer_sdp)
            .await
            .expect("offerer set remote");

        timeout(Duration::from_secs(10), off_conn_rx.recv())
            .await
            .expect("offerer connected");
        timeout(Duration::from_secs(10), ans_conn_rx.recv())
            .await
            .expect("answerer connected");

        // ---- send ----
        let payload = bytes::Bytes::from(vec![0xABu8; 200]);
        let mut ticker = interval(Duration::from_millis(2));
        for sequence_number in 1..=MEDIA_PACKETS_TO_SEND {
            video_track
                .write_rtp(rtp::packet::Packet {
                    header: rtp::header::Header {
                        version: 2,
                        payload_type: VP8_PT,
                        // The identity under test. One per packet, from 1, so a reported sequence
                        // number names exactly one send.
                        sequence_number,
                        timestamp: u32::from(sequence_number) * 3000,
                        ssrc: MEDIA_SSRC,
                        ..Default::default()
                    },
                    payload: payload.clone(),
                })
                .await
                .expect("write_rtp");
            ticker.tick().await;
        }

        // Give the tail of the stream time to land: the last block's repair packet is emitted
        // after its tenth media packet, so recovery of a loss in that block necessarily trails the
        // last send.
        common::sleep(Duration::from_secs(2)).await;

        offerer.close().await.expect("close offerer");
        answerer.close().await.expect("close answerer");

        // ---- collect ----
        let dropped: BTreeSet<u16> = dropped_rx.try_iter().collect();
        let mut arrived = BTreeSet::new();
        let mut recovered = BTreeSet::new();
        for (sequence_number, was_recovered) in arrivals_rx.try_iter() {
            if was_recovered {
                recovered.insert(sequence_number);
            } else {
                arrived.insert(sequence_number);
            }
        }

        // Guards the rest: with nothing dropped, every assertion below holds vacuously and the
        // test would pass on a build where FEC does nothing whatsoever.
        let expected_drops = usize::from(MEDIA_PACKETS_TO_SEND) / DROP_ONE_IN as usize;
        assert_eq!(
            expected_drops,
            dropped.len(),
            "the drop filter should have discarded one packet in {DROP_ONE_IN}, got {dropped:?}"
        );

        // The claim: every packet the sender discarded was rebuilt by the decoder and handed on.
        assert!(
            dropped.is_subset(&recovered),
            "these dropped sequence numbers were never recovered: {:?}\n\
             dropped:   {dropped:?}\n\
             recovered: {recovered:?}",
            dropped.difference(&recovered).collect::<Vec<_>>()
        );

        // And nothing else went astray, so the recovery above is the whole story rather than one
        // effect among several.
        let all: BTreeSet<u16> = (1..=MEDIA_PACKETS_TO_SEND).collect();
        let delivered: BTreeSet<u16> = arrived.union(&recovered).copied().collect();
        assert_eq!(
            all,
            delivered,
            "packets neither delivered nor recovered: {:?}",
            all.difference(&delivered).collect::<Vec<_>>()
        );

        // What was rebuilt that had not been lost.
        //
        // This should be empty and is not: the first packet of the stream is always rebuilt as
        // well, duplicating one the receiver already had. The cause is not in the codec. A remote
        // stream is bound to the interceptors only once its codec can be resolved from an arriving
        // RTP payload type (`rtc`'s `endpoint.rs`, `find_track_id_by_ssrc`), and the endpoint sits
        // application-ward of the chain — so the packet that triggers the bind has already
        // traversed the chain by the time the bind happens. The decoder therefore never sees
        // packet one, finds it missing when the first repair packet arrives, and rebuilds it.
        //
        // Pinned rather than tolerated. If the artifact ever spreads beyond that first packet this
        // fails, and when the bind ordering is fixed the `is_empty` case starts holding and this
        // can be tightened to set equality.
        let spurious: BTreeSet<u16> = recovered.difference(&dropped).copied().collect();
        assert!(
            spurious.is_empty() || spurious == BTreeSet::from([1]),
            "packets were rebuilt that had not been lost: {spurious:?}\n\
             only sequence number 1 is a known artifact of the late remote-stream bind"
        );
    });
}
