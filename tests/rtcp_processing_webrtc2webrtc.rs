//! Async webrtc integration test for RTCP processing.
//!
//! This is the async-API port of the sansio `rtc/rtc/tests/rtcp_processing_interop.rs`.
//!
//! With `Registry::with_rtcp_readable()` on the chain (inbound RTCP is otherwise consumed by
//! the interceptors before the application can see it), it verifies **both** directions of RTCP
//! delivery:
//!
//! * **Receiver side** — the receiving peer surfaces RTCP about the stream it is receiving
//!   (the sender's Sender Reports) via `TrackRemoteEvent::OnRtcpPacket`.
//! * **Sender side** — the sending peer surfaces RTCP feedback about its own sent stream via the
//!   new `TrackLocal::poll` → `TrackLocalEvent::OnRtcpPacket`. This relies on the endpoint
//!   handler tagging inbound RTCP with a *sender's* track id and the driver routing it to the
//!   local track.
//!
//! The sending peer additionally carries an [`RtcpForwarderInterceptor`], which narrows what
//! reaches the application to keyframe requests. The receiver drives real PLIs, and the Receiver
//! Reports it also emits are what proves the drop half: they reach the chain and must not appear.
//!
//! Topology: two async webrtc peers. The offerer sends a VP8 track; the answerer receives
//! it. (See `examples/rtcp-processing` for the single-peer version.)

use rtc::sansio;
use rtc::shared::error::Error;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use rtc::interceptor::{Interceptor, Packet, Registry, StreamInfo, TaggedPacket};
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_VP8, MediaEngine};
use rtc::rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use rtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use rtc::rtcp::receiver_report::ReceiverReport;
use rtc::rtcp::reception_report::ReceptionReport;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters, RTCRtpEncodingParameters,
    RtpCodecKind,
};

use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::media_stream::track_local::{TrackLocal, TrackLocalEvent};
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel};

mod common;
use common::{block_on, interval, runtime, timeout};

// ============================================================================
// RTCP Forwarder Interceptor — surfaces inbound keyframe requests, drops the rest.
// ============================================================================

/// Builder for the [`RtcpForwarderInterceptor`].
#[derive(Default)]
struct RtcpForwarderBuilder;

impl RtcpForwarderBuilder {
    fn new() -> Self {
        Self
    }

    fn build(self) -> RtcpForwarderInterceptor {
        RtcpForwarderInterceptor {
            read_queue: VecDeque::new(),
            write_queue: VecDeque::new(),
        }
    }
}

/// Passes inbound keyframe requests — PLI and FIR — on to the application, and drops every other
/// inbound RTCP packet.
///
/// The rest of the inbound RTCP is for the interceptors: a receiver report feeds the sender
/// statistics, a NACK is answered by the responder, transport-wide feedback drives the bandwidth
/// estimate. A keyframe request is the exception — only the application can answer it, because
/// only the application knows what is producing the stream.
///
/// **Last** in the chain, so every interceptor has already seen the whole of the inbound RTCP
/// before this one narrows it down; and it needs `Registry::with_rtcp_readable`, because what it
/// passes on still has to get past the stage that ends the inbound RTCP path.
struct RtcpForwarderInterceptor {
    read_queue: VecDeque<TaggedPacket>,
    write_queue: VecDeque<TaggedPacket>,
}

/// Whether an RTCP packet is a request for a keyframe.
fn is_keyframe_request(packet: &Box<dyn rtc::rtcp::Packet>) -> bool {
    let payload = packet.as_any();
    payload.is::<PictureLossIndication>() || payload.is::<FullIntraRequest>()
}

impl sansio::Protocol<TaggedPacket, TaggedPacket, ()> for RtcpForwarderInterceptor {
    type Rout = TaggedPacket;
    type Wout = TaggedPacket;
    type Eout = ();
    type Error = Error;
    type Time = std::time::Instant;

    fn handle_read(&mut self, mut msg: TaggedPacket) -> Result<(), Self::Error> {
        if let Packet::Rtcp(packets) = &msg.message.packet {
            let requests: Vec<Box<dyn rtc::rtcp::Packet>> = packets
                .iter()
                .filter(|packet| is_keyframe_request(packet))
                .cloned()
                .collect();
            if requests.is_empty() {
                // Not the application's business, and the interceptors have already acted on it.
                return Ok(());
            }
            msg.message.packet = Packet::Rtcp(requests);
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
}

impl Interceptor for RtcpForwarderInterceptor {
    fn bind_local_stream(&mut self, _info: &StreamInfo) {}
    fn unbind_local_stream(&mut self, _info: &StreamInfo) {}
    fn bind_remote_stream(&mut self, _info: &StreamInfo) {}
    fn unbind_remote_stream(&mut self, _info: &StreamInfo) {}
}

// ============================================================================
// Event handlers
// ============================================================================

struct OffererHandler {
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for OffererHandler {
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

struct AnswererHandler {
    runtime: Arc<dyn Runtime>,
    gather_complete_tx: Sender<()>,
    connected_tx: Sender<()>,
    rtcp_count: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for AnswererHandler {
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

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        // Read RTCP about the received stream. Without `with_rtcp_readable` these
        // OnRtcpPacket events never arrive (the interceptors consume the RTCP first).
        let rtcp_count = Arc::clone(&self.rtcp_count);
        let poll_track = Arc::clone(&track);
        self.runtime.spawn(Box::pin(async move {
            while let Some(evt) = poll_track.poll().await {
                match evt {
                    TrackRemoteEvent::OnRtcpPacket(_packets) => {
                        rtcp_count.fetch_add(1, Ordering::SeqCst);
                    }
                    TrackRemoteEvent::OnEnded => break,
                    _ => {}
                }
            }
        }));

        // Periodically request a keyframe (PLI) from the sender, alongside a Receiver Report
        // about the same stream. This deterministically gives the sender inbound RTCP feedback
        // about its own stream, which it reads via TrackLocal::poll — without relying on periodic
        // report generation. The report is the packet the sender's forwarder has to *drop*, and
        // sending it explicitly is what makes that assertion mean something.
        let media_ssrc = track.ssrcs().await.first().copied();
        self.runtime.spawn(Box::pin(async move {
            let mut ticker = interval(Duration::from_millis(200));
            loop {
                let _ = ticker.tick().await;
                let Some(media_ssrc) = media_ssrc else {
                    continue;
                };
                let pli = PictureLossIndication {
                    sender_ssrc: 0,
                    media_ssrc,
                };
                let report = ReceiverReport {
                    ssrc: 0,
                    reports: vec![ReceptionReport {
                        ssrc: media_ssrc,
                        ..Default::default()
                    }],
                    ..Default::default()
                };
                if track
                    .write_rtcp(vec![Box::new(pli), Box::new(report)])
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// A media engine with a single VP8 codec plus the default interceptors, on a chain whose
/// inbound RTCP is readable by the application.
///
/// `keyframe_requests_only` adds the [`RtcpForwarderInterceptor`], which narrows what the
/// application sees to PLI and FIR. Only a peer that is *sending* can be asked for a keyframe, so
/// the receiving peer leaves it off and reads the reports it is about.
async fn build_peer(
    runtime: Arc<dyn Runtime>,
    handler: Arc<dyn PeerConnectionEventHandler>,
    keyframe_requests_only: bool,
) -> Arc<dyn PeerConnection> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
                ..Default::default()
            },
            RtpCodecKind::Video,
        )
        .expect("register VP8");

    // Inbound RTCP is for the interceptors unless the chain asks for it; this test is about
    // reading it, in both directions.
    let registry =
        register_default_interceptors(Registry::new().with_rtcp_readable(), &mut media_engine)
            .expect("default interceptors");

    // Application-most, so every interceptor has already seen the whole of the inbound RTCP.
    let registry = if keyframe_requests_only {
        registry.with(RtcpForwarderBuilder::new().build())
    } else {
        registry
    };

    let pc = PeerConnectionBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_runtime(runtime)
        .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
        .build()
        .await
        .expect("build peer connection");
    Arc::new(pc) as Arc<dyn PeerConnection>
}

// ============================================================================
// Test
// ============================================================================

#[test]
fn test_rtcp_processing_webrtc2webrtc() {
    block_on(async {
        let runtime = runtime();

        const VIDEO_SSRC: u32 = 0x00DE_CAFE;

        // Offerer that SENDS a VP8 track.
        let (off_gather_tx, mut off_gather_rx) = channel::<()>(1);
        let (off_conn_tx, mut off_conn_rx) = channel::<()>(1);
        let offerer = build_peer(
            runtime.clone(),
            Arc::new(OffererHandler {
                gather_complete_tx: off_gather_tx,
                connected_tx: off_conn_tx,
            }),
            // The offerer sends, so it is the peer that gets asked for keyframes.
            true,
        )
        .await;

        let video_track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
            "rtcp-test-stream".to_owned(),
            "rtcp-test-video".to_owned(),
            "rtcp-test-label".to_owned(),
            RtpCodecKind::Video,
            vec![RTCRtpEncodingParameters {
                rtp_coding_parameters: RTCRtpCodingParameters {
                    ssrc: Some(VIDEO_SSRC),
                    ..Default::default()
                },
                codec: RTCRtpCodec {
                    mime_type: MIME_TYPE_VP8.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                ..Default::default()
            }],
        )));
        offerer
            .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal>)
            .await
            .expect("add video track");

        // Answerer that RECEIVES the track and counts RTCP about it.
        let rtcp_count = Arc::new(AtomicU32::new(0));
        let (ans_gather_tx, mut ans_gather_rx) = channel::<()>(1);
        let (ans_conn_tx, mut ans_conn_rx) = channel::<()>(1);
        let answerer = build_peer(
            runtime.clone(),
            Arc::new(AnswererHandler {
                runtime: runtime.clone(),
                gather_complete_tx: ans_gather_tx,
                connected_tx: ans_conn_tx,
                rtcp_count: Arc::clone(&rtcp_count),
            }),
            // The answerer only receives; no keyframe request can ever reach it, so it reads the
            // reports about the stream it is receiving instead.
            false,
        )
        .await;

        // Offer / answer with non-trickle ICE (wait for gathering to complete).
        let offer = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer).await.unwrap();
        timeout(Duration::from_secs(5), off_gather_rx.recv())
            .await
            .expect("offerer gather");
        let offer_sdp = offerer.local_description().await.unwrap();

        answerer.set_remote_description(offer_sdp).await.unwrap();
        let answer = answerer.create_answer(None).await.unwrap();
        answerer.set_local_description(answer).await.unwrap();
        timeout(Duration::from_secs(5), ans_gather_rx.recv())
            .await
            .expect("answerer gather");
        let answer_sdp = answerer.local_description().await.unwrap();

        offerer.set_remote_description(answer_sdp).await.unwrap();

        timeout(Duration::from_secs(5), off_conn_rx.recv())
            .await
            .expect("offerer connect");
        timeout(Duration::from_secs(5), ans_conn_rx.recv())
            .await
            .expect("answerer connect");

        // Stream RTP so the answerer keeps receiving and the offerer keeps emitting SRs.
        let stream_track = Arc::clone(&video_track);
        runtime.spawn(Box::pin(async move {
            let mut ticker = interval(Duration::from_millis(20));
            for seq in 0u16..500 {
                let packet = rtc::rtp::packet::Packet {
                    header: rtc::rtp::header::Header {
                        version: 2,
                        payload_type: 96,
                        sequence_number: seq,
                        timestamp: (seq as u32).wrapping_mul(3000),
                        ssrc: VIDEO_SSRC,
                        ..Default::default()
                    },
                    payload: bytes::Bytes::from(vec![0xAAu8; 100]),
                };
                let _ = stream_track.write_rtp(packet).await;
                let _ = ticker.tick().await;
            }
        }));

        // The offerer reads RTCP feedback about its OWN sent track via the local-track poll
        // API — the keyframe requests the answerer sends. This exercises `TrackLocal::poll` and
        // the endpoint handler's sender-side RTCP surfacing.
        //
        // The answerer sends a Receiver Report alongside every PLI, so non-keyframe RTCP
        // certainly reaches this chain. `not_a_keyframe_request` counts any that gets past the
        // forwarder — the drop half of "PLI and FIR only", asserted against traffic known to be
        // arriving rather than against the absence of events.
        let local_rtcp_count = Arc::new(AtomicU32::new(0));
        let not_a_keyframe_request = Arc::new(AtomicU32::new(0));
        {
            let local_rtcp_count = Arc::clone(&local_rtcp_count);
            let not_a_keyframe_request = Arc::clone(&not_a_keyframe_request);
            let poll_track = Arc::clone(&video_track);
            runtime.spawn(Box::pin(async move {
                while let Some(evt) = poll_track.poll().await {
                    if let TrackLocalEvent::OnRtcpPacket(packets) = evt {
                        for packet in &packets {
                            if !is_keyframe_request(packet) {
                                not_a_keyframe_request.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                        local_rtcp_count.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }));
        }

        // The answerer should surface RTCP about the received stream (receiver side), and the
        // offerer should surface RTCP about its sent stream (sender side, the new API).
        let mut poll = interval(Duration::from_millis(100));
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            if rtcp_count.load(Ordering::SeqCst) >= 2
                && local_rtcp_count.load(Ordering::SeqCst) >= 1
            {
                break;
            }
            let _ = poll.tick().await;
        }

        let received = rtcp_count.load(Ordering::SeqCst);
        let local_received = local_rtcp_count.load(Ordering::SeqCst);
        assert!(
            received >= 2,
            "answerer should surface RTCP about the received stream (got {received}); \
             without `with_rtcp_readable` the interceptors consume it"
        );
        assert!(
            local_received >= 1,
            "offerer should surface RTCP feedback about its SENT stream via TrackLocal::poll \
             (got {local_received})"
        );
        let leaked = not_a_keyframe_request.load(Ordering::SeqCst);
        assert_eq!(
            0, leaked,
            "the forwarder passes PLI and FIR only, but let {leaked} other RTCP packet(s) through"
        );

        offerer.close().await.ok();
        answerer.close().await.ok();
    });
}
