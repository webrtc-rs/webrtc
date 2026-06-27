//! Integration test for PeerConnection renegotiation.
//!
//! This test verifies that we can renegotiate a connection (e.g. by adding
//! a track after the initial connection is established) without generating
//! duplicate mid attributes in the renegotiation offer SDP.

use rtc::media_stream::MediaStreamTrack;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpEncodingParameters, RtpCodecKind};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use webrtc::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCIceGatheringState, RTCPeerConnectionState, RTCSessionDescription,
};
use webrtc::runtime::{block_on, channel, default_runtime, timeout};

// ── Event Handlers ────────────────────────────────────────────────────────────

struct OffererHandler {
    gather_complete_tx: webrtc::runtime::Sender<()>,
    connected_tx: webrtc::runtime::Sender<()>,
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
    gather_complete_tx: webrtc::runtime::Sender<()>,
    connected_tx: webrtc::runtime::Sender<()>,
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
}

// ── Helper functions ──────────────────────────────────────────────────────────

fn new_video_track(stream_id: &str, track_id: &str, _ssrc: u32) -> Arc<TrackLocalStaticRTP> {
    Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        stream_id.to_owned(),
        track_id.to_owned(),
        format!("track-{track_id}"),
        RtpCodecKind::Video,
        vec![RTCRtpEncodingParameters {
            codec: RTCRtpCodec {
                mime_type: "video/VP8".to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: String::new(),
                rtcp_feedback: vec![],
            },
            ..Default::default()
        }],
    )))
}

fn collect_mids(sdp: &RTCSessionDescription) -> Vec<String> {
    let parsed = sdp.unmarshal().expect("failed to parse SDP");
    parsed
        .media_descriptions
        .iter()
        .filter_map(|m| m.attribute("mid").and_then(|v| v.map(|s| s.to_owned())))
        .collect()
}

// ── Test Case ─────────────────────────────────────────────────────────────────

#[test]
fn test_renegotiation_no_duplicate_mids() {
    block_on(async {
        let runtime = default_runtime().expect("no runtime");

        let mut me = MediaEngine::default();
        me.register_default_codecs().unwrap();
        let me2 = me.clone();

        // --- Offerer ---
        let (off_gather_tx, mut off_gather_rx) = channel(1);
        let (off_conn_tx, mut off_conn_rx) = channel(1);
        let offerer = PeerConnectionBuilder::new()
            .with_media_engine(me)
            .with_handler(Arc::new(OffererHandler {
                gather_complete_tx: off_gather_tx,
                connected_tx: off_conn_tx,
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .unwrap();
        let offerer = Arc::new(offerer);

        // Add first video track
        let track1 = new_video_track("stream-1", "video-1", 11111);
        offerer.add_track(track1).await.unwrap();

        // --- Answerer ---
        let (ans_gather_tx, mut ans_gather_rx) = channel(1);
        let (ans_conn_tx, mut ans_conn_rx) = channel(1);
        let answerer = PeerConnectionBuilder::new()
            .with_media_engine(me2)
            .with_handler(Arc::new(AnswererHandler {
                gather_complete_tx: ans_gather_tx,
                connected_tx: ans_conn_tx,
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .unwrap();
        let answerer = Arc::new(answerer);

        // 1. First offer/answer exchange
        let offer1 = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer1).await.unwrap();
        timeout(Duration::from_secs(5), off_gather_rx.recv())
            .await
            .unwrap();

        let offer_sdp1 = offerer.local_description().await.unwrap();
        answerer.set_remote_description(offer_sdp1).await.unwrap();

        let answer1 = answerer.create_answer(None).await.unwrap();
        answerer.set_local_description(answer1).await.unwrap();
        timeout(Duration::from_secs(5), ans_gather_rx.recv())
            .await
            .unwrap();

        let answer_sdp1 = answerer.local_description().await.unwrap();
        offerer.set_remote_description(answer_sdp1).await.unwrap();

        // Wait for connection to establish
        timeout(Duration::from_secs(5), off_conn_rx.recv())
            .await
            .unwrap();
        timeout(Duration::from_secs(5), ans_conn_rx.recv())
            .await
            .unwrap();

        // 2. Add a second video track and renegotiate
        let track2 = new_video_track("stream-2", "video-2", 22222);
        offerer.add_track(track2).await.unwrap();

        let offer2 = offerer.create_offer(None).await.unwrap();
        let mids = collect_mids(&offer2);
        let unique: HashSet<_> = mids.iter().collect();
        assert_eq!(
            mids.len(),
            unique.len(),
            "renegotiation offer has duplicate mids: {:?}",
            mids
        );
        assert_eq!(
            mids.len(),
            2,
            "expected 2 media sections, got {}: {:?}",
            mids.len(),
            mids
        );

        // Close peer connections
        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    });
}
