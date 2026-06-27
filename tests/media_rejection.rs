//! Integration test for media rejection.
//!
//! This test verifies that if the offerer offers both video and audio,
//! but the answerer only registers video codecs, the answerer will
//! correctly reject the audio section (generating an SDP answer with
//! port=0 for the audio m-line), and a video-only connection will
//! be successfully established.

use rtc::peer_connection::configuration::media_engine::MIME_TYPE_VP8;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use std::sync::Arc;
use std::time::Duration;

use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCIceGatheringState, RTCPeerConnectionState,
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

// ── Test Case ─────────────────────────────────────────────────────────────────

#[test]
fn test_media_rejection_audio_rejected_video_connected() {
    block_on(async {
        let runtime = default_runtime().expect("no runtime");

        // Offerer MediaEngine: Video + Audio (defaults)
        let mut offerer_media = MediaEngine::default();
        offerer_media.register_default_codecs().unwrap();

        // Answerer MediaEngine: Video ONLY
        let mut answerer_media = MediaEngine::default();
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
        answerer_media
            .register_codec(video_codec, RtpCodecKind::Video)
            .unwrap();

        // Build Offerer PeerConnection
        let (off_gather_tx, mut off_gather_rx) = channel(1);
        let (off_conn_tx, mut off_conn_rx) = channel(1);
        let offerer = PeerConnectionBuilder::new()
            .with_media_engine(offerer_media)
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

        // Build Answerer PeerConnection
        let (ans_gather_tx, mut ans_gather_rx) = channel(1);
        let (ans_conn_tx, mut ans_conn_rx) = channel(1);
        let answerer = PeerConnectionBuilder::new()
            .with_media_engine(answerer_media)
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

        // Add Video & Audio transceivers to offerer
        offerer
            .add_transceiver_from_kind(RtpCodecKind::Video, None)
            .await
            .unwrap();
        offerer
            .add_transceiver_from_kind(RtpCodecKind::Audio, None)
            .await
            .unwrap();

        // 1. Offerer create offer and set local description
        let offer = offerer.create_offer(None).await.unwrap();
        offerer.set_local_description(offer).await.unwrap();

        // 2. Wait for offerer ICE gathering
        timeout(Duration::from_secs(5), off_gather_rx.recv())
            .await
            .unwrap();
        let offer_sdp = offerer.local_description().await.unwrap();

        // 3. Answerer set remote description
        answerer.set_remote_description(offer_sdp).await.unwrap();

        // 4. Answerer create answer and set local description
        let answer = answerer.create_answer(None).await.unwrap();
        answerer.set_local_description(answer).await.unwrap();

        // 5. Wait for answerer ICE gathering
        timeout(Duration::from_secs(5), ans_gather_rx.recv())
            .await
            .unwrap();
        let answer_sdp = answerer.local_description().await.unwrap();

        // 6. Offerer set remote description
        offerer
            .set_remote_description(answer_sdp.clone())
            .await
            .unwrap();

        // Assert that the SDP answer contains the video section (accepted) and audio section (rejected)
        let sdp_lines: Vec<&str> = answer_sdp.sdp.lines().collect();

        let video_mline = sdp_lines.iter().find(|l| l.starts_with("m=video"));
        assert!(
            video_mline.is_some(),
            "SDP Answer must contain a video m-line"
        );
        let video_mline = video_mline.unwrap();
        assert!(
            !video_mline.contains("m=video 0"),
            "Video must NOT be rejected: {}",
            video_mline
        );

        let audio_mline = sdp_lines.iter().find(|l| l.starts_with("m=audio"));
        assert!(
            audio_mline.is_some(),
            "SDP Answer must contain an audio m-line"
        );
        let audio_mline = audio_mline.unwrap();
        assert!(
            audio_mline.contains("m=audio 0"),
            "Audio must be rejected with port=0: {}",
            audio_mline
        );

        // Wait for connection to establish (since video is accepted, connection will succeed)
        timeout(Duration::from_secs(5), off_conn_rx.recv())
            .await
            .unwrap();
        timeout(Duration::from_secs(5), ans_conn_rx.recv())
            .await
            .unwrap();

        // Close peer connections
        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    });
}
