//! Integration test for PeerConnection get_stats() API.
//!
//! This test establishes a connection between two high-level PeerConnections,
//! sends some data channel messages, and verifies that get_stats() returns
//! valid statistics report containing connection, data channel, transport,
//! and ICE candidate pair metrics.

use rtc::statistics::StatsSelector;
use std::sync::Arc;
use std::time::{Duration, Instant};

use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{block_on, channel, default_runtime, sleep, timeout};

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
    data_channel_tx: webrtc::runtime::Sender<Arc<dyn DataChannel>>,
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

    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        let _ = self.data_channel_tx.try_send(data_channel);
    }
}

// ── Test Case ─────────────────────────────────────────────────────────────────

#[test]
fn test_peer_connection_statistics() {
    block_on(async {
        let runtime = default_runtime().expect("no runtime");

        // --- Offerer ---
        let (off_gather_tx, mut off_gather_rx) = channel(1);
        let (off_conn_tx, mut off_conn_rx) = channel(1);
        let offerer = PeerConnectionBuilder::new()
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

        // --- Answerer ---
        let (ans_gather_tx, mut ans_gather_rx) = channel(1);
        let (ans_conn_tx, mut ans_conn_rx) = channel(1);
        let (dc_tx, mut dc_rx) = channel(1);
        let answerer = PeerConnectionBuilder::new()
            .with_handler(Arc::new(AnswererHandler {
                gather_complete_tx: ans_gather_tx,
                connected_tx: ans_conn_tx,
                data_channel_tx: dc_tx,
            }))
            .with_runtime(runtime.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .unwrap();
        let answerer = Arc::new(answerer);

        // Create data channel on offerer
        let offer_dc = offerer.create_data_channel("stats-dc", None).await.unwrap();
        let offer_dc_clone = offer_dc.clone();

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
        offerer.set_remote_description(answer_sdp).await.unwrap();

        // Wait for connection to establish
        timeout(Duration::from_secs(5), off_conn_rx.recv())
            .await
            .unwrap();
        timeout(Duration::from_secs(5), ans_conn_rx.recv())
            .await
            .unwrap();

        // Wait for data channel to open
        let ans_dc = timeout(Duration::from_secs(5), dc_rx.recv())
            .await
            .unwrap()
            .unwrap();

        // Exchange some messages to generate stats
        runtime.spawn(Box::pin(async move {
            loop {
                if let Some(DataChannelEvent::OnOpen) = offer_dc_clone.poll().await {
                    break;
                }
            }
            offer_dc_clone.send_text("Hello stats!").await.unwrap();
        }));

        loop {
            if let Some(DataChannelEvent::OnMessage(msg)) = ans_dc.poll().await {
                assert_eq!(
                    String::from_utf8(msg.data.to_vec()).unwrap(),
                    "Hello stats!"
                );
                break;
            }
        }

        // Wait a brief moment for statistics to settle
        sleep(Duration::from_millis(100)).await;

        // Query stats from offerer
        let offerer_stats = offerer.get_stats(Instant::now(), StatsSelector::None).await;
        assert!(
            !offerer_stats.is_empty(),
            "Offerer stats report should not be empty"
        );

        // Verify PeerConnection stats
        let pc_stats = offerer_stats
            .peer_connection()
            .expect("PeerConnection stats missing");
        assert_eq!(pc_stats.data_channels_opened, 1);

        // Verify DataChannel stats
        let dc_stats_list: Vec<_> = offerer_stats.data_channels().collect();
        assert_eq!(
            dc_stats_list.len(),
            1,
            "Expected 1 data channel stats entry"
        );
        let dc_stats = dc_stats_list[0];
        assert_eq!(dc_stats.label, "stats-dc");
        assert!(dc_stats.messages_sent > 0, "Expected messages_sent > 0");

        // Verify Transport stats
        let transport_stats = offerer_stats.transport().expect("Transport stats missing");
        assert!(
            transport_stats.bytes_sent > 0,
            "Expected transport bytes_sent > 0"
        );

        // Verify ICE candidate pairs stats
        let pair_stats_list: Vec<_> = offerer_stats.candidate_pairs().collect();
        assert!(!pair_stats_list.is_empty(), "Candidate pair stats missing");

        // Query stats from answerer
        let answerer_stats = answerer
            .get_stats(Instant::now(), StatsSelector::None)
            .await;
        assert!(
            !answerer_stats.is_empty(),
            "Answerer stats report should not be empty"
        );
        let ans_pc_stats = answerer_stats
            .peer_connection()
            .expect("Answerer PeerConnection stats missing");
        assert_eq!(ans_pc_stats.data_channels_opened, 1);

        // Close peer connections
        offerer.close().await.unwrap();
        answerer.close().await.unwrap();
    });
}
