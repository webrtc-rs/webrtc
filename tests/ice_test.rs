//! Integration tests for ICE functionality

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use webrtc::peer_connection::PeerConnection;
use webrtc::peer_connection_event_handler::PeerConnectionEventHandler;
use webrtc::runtime::Mutex;
use webrtc::runtime::sleep;
use webrtc::{
    MediaEngine, RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceCandidateType,
    RTCIceGatheringState, RTCIceServer, RTCPeerConnectionIceEvent,
};

#[derive(Clone)]
struct IceTestHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for IceTestHandler {}

#[derive(Clone)]
struct IceGatheringHandler {
    candidate_received: Arc<AtomicBool>,
    gathering_complete: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for IceGatheringHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        eprintln!("✅ Received ICE candidate event: {:?}", event.candidate);
        self.candidate_received.store(true, Ordering::SeqCst);
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        eprintln!("ICE gathering state: {:?}", state);
        if state == RTCIceGatheringState::Complete {
            self.gathering_complete.store(true, Ordering::SeqCst);
        }
    }
}

// Handler that tracks candidate types for STUN testing
struct CandidateTypeTracker {
    candidates: Arc<Mutex<Vec<RTCIceCandidateType>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for CandidateTypeTracker {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let typ = event.candidate.typ;
        println!(
            "✅ Received {:?} candidate: {} (port {})",
            typ, event.candidate.address, event.candidate.port
        );
        self.candidates.lock().await.push(typ);
    }
}

#[tokio::test]
async fn test_add_ice_candidate() {
    // Test adding remote ICE candidates
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("Failed to register codecs");

    let config = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine)
        .build();

    let handler = Arc::new(IceTestHandler);

    let pc_a =
        PeerConnection::new(config, handler.clone()).expect("Failed to create peer connection A");

    let mut media_engine_b = MediaEngine::default();
    media_engine_b
        .register_default_codecs()
        .expect("Failed to register codecs");
    let config_b = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine_b)
        .build();

    let pc_b = PeerConnection::new(config_b, handler).expect("Failed to create peer connection B");

    // Add track to trigger negotiation
    let track = rtc::media_stream::MediaStreamTrack::new(
        "stream".to_string(),
        "video".to_string(),
        "track".to_string(),
        rtc::rtp_transceiver::rtp_sender::RtpCodecKind::Video,
        vec![],
    );
    pc_a.add_track(track).await.expect("Failed to add track");

    // Create offer/answer
    let offer = pc_a
        .create_offer(None)
        .await
        .expect("Failed to create offer");
    pc_a.set_local_description(offer.clone())
        .await
        .expect("Failed to set local description");
    pc_b.set_remote_description(offer)
        .await
        .expect("Failed to set remote description");

    let answer = pc_b
        .create_answer(None)
        .await
        .expect("Failed to create answer");
    pc_b.set_local_description(answer.clone())
        .await
        .expect("Failed to set local description");
    pc_a.set_remote_description(answer)
        .await
        .expect("Failed to set remote description");

    // Now we can add ICE candidates
    let candidate_init = RTCIceCandidateInit {
        candidate: "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host".to_string(),
        sdp_mid: Some("0".to_string()),
        sdp_mline_index: Some(0),
        username_fragment: None,
        url: None,
    };

    // Should succeed after remote description is set
    let result = pc_a.add_ice_candidate(candidate_init);
    assert!(result.await.is_ok(), "Adding ICE candidate should succeed");
}

#[tokio::test]
async fn test_restart_ice() {
    // Test ICE restart API
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("Failed to register codecs");

    let config = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine)
        .build();

    let handler = Arc::new(IceTestHandler);

    let pc = PeerConnection::new(config, handler).expect("Failed to create peer connection");

    // Add track
    let track = rtc::media_stream::MediaStreamTrack::new(
        "stream".to_string(),
        "video".to_string(),
        "track".to_string(),
        rtc::rtp_transceiver::rtp_sender::RtpCodecKind::Video,
        vec![],
    );
    pc.add_track(track).await.expect("Failed to add track");

    let offer1 = pc
        .create_offer(None)
        .await
        .expect("Failed to create first offer");
    let sdp1 = offer1.sdp.clone();

    pc.set_local_description(offer1)
        .await
        .expect("Failed to set local description");

    // Trigger ICE restart
    pc.restart_ice().await.expect("Failed to restart ICE");

    // Create new offer - should have different ICE credentials
    let offer2 = pc
        .create_offer(None)
        .await
        .expect("Failed to create second offer");
    let sdp2 = offer2.sdp.clone();

    // The SDPs should be different (new ICE credentials)
    assert_ne!(sdp1, sdp2, "ICE restart should generate new credentials");
}

#[tokio::test]
async fn test_automatic_host_candidate_gathering() {
    // Test that host candidates are automatically gathered when setLocalDescription is called
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("Failed to register codecs");

    let config = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine)
        .build();

    let handler = Arc::new(IceGatheringHandler {
        candidate_received: Arc::new(AtomicBool::new(false)),
        gathering_complete: Arc::new(AtomicBool::new(false)),
    });
    let candidate_flag = handler.candidate_received.clone();
    let _complete_flag = handler.gathering_complete.clone();

    let pc = PeerConnection::new(config, handler).expect("Failed to create peer connection");

    // Bind socket and spawn driver (required for event processing)
    let driver = pc
        .bind("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .await
        .expect("Failed to bind");

    let _driver_handle = tokio::spawn(async move { driver.run().await });

    // Add track to create media
    let track = rtc::media_stream::MediaStreamTrack::new(
        "stream".to_string(),
        "video".to_string(),
        "track".to_string(),
        rtc::rtp_transceiver::rtp_sender::RtpCodecKind::Video,
        vec![],
    );
    pc.add_track(track).await.expect("Failed to add track");

    // Create and set local description - this should trigger gathering
    let offer = pc.create_offer(None).await.expect("Failed to create offer");
    pc.set_local_description(offer)
        .await
        .expect("Failed to set local description");

    // Give the driver time to process events
    sleep(Duration::from_millis(1000)).await;

    // Verify that a host candidate was gathered
    assert!(
        candidate_flag.load(Ordering::SeqCst),
        "Should have received at least one ICE candidate"
    );

    println!("✅ Host candidate gathering successful!");
}

#[tokio::test]
async fn test_stun_gathering_with_google_stun() {
    // Test STUN gathering with Google's public STUN server
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("Failed to register codecs");

    let ice_servers = vec![RTCIceServer {
        urls: vec!["stun:stun.l.google.com:19302".to_string()],
        username: String::new(),
        credential: String::new(),
    }];

    let config = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine)
        .with_ice_servers(ice_servers)
        .build();

    // Track candidate types to verify we get both host and srflx
    let candidates = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(CandidateTypeTracker {
        candidates: candidates.clone(),
    });

    let pc = PeerConnection::new(config, handler).expect("Failed to create peer connection");

    // Bind socket and spawn driver
    let driver = pc
        .bind("0.0.0.0:0".parse::<std::net::SocketAddr>().unwrap())
        .await
        .expect("Failed to bind");

    let _driver_handle = tokio::spawn(async move { driver.run().await });

    // Add track to create media
    let track = rtc::media_stream::MediaStreamTrack::new(
        "stream".to_string(),
        "video".to_string(),
        "track".to_string(),
        rtc::rtp_transceiver::rtp_sender::RtpCodecKind::Video,
        vec![],
    );
    pc.add_track(track).await.expect("Failed to add track");

    // Create and set local description - this should trigger gathering
    let offer = pc.create_offer(None).await.expect("Failed to create offer");
    pc.set_local_description(offer)
        .await
        .expect("Failed to set local description");

    // Wait for both host and STUN candidates to arrive
    // We expect at least: 1 host + 1 srflx = 2 candidates
    println!("⏳ Waiting for ICE candidates...");
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(15);

    loop {
        let count = candidates.lock().await.len();
        if count >= 2 {
            println!("✅ Received {} candidates in {:?}", count, start.elapsed());
            break;
        }
        if start.elapsed() > timeout {
            let count = candidates.lock().await.len();
            panic!("Timeout waiting for candidates. Got {} candidates", count);
        }
        sleep(Duration::from_millis(100)).await;
    }

    // Verify we got both host and srflx candidates
    let gathered: Vec<RTCIceCandidateType> = candidates.lock().await.clone();
    println!("📊 Gathered {} candidates: {:?}", gathered.len(), gathered);

    // Should have at least 2 candidates: host + srflx
    assert!(
        gathered.len() >= 2,
        "Expected at least 2 candidates (host + srflx), got {}",
        gathered.len()
    );

    // Verify we have a host candidate
    let has_host = gathered.iter().any(|t| *t == RTCIceCandidateType::Host);
    assert!(has_host, "Missing host candidate");

    // Verify we have an srflx candidate from STUN
    let has_srflx = gathered.iter().any(|t| *t == RTCIceCandidateType::Srflx);
    assert!(
        has_srflx,
        "Missing srflx candidate - STUN gathering may have failed"
    );

    println!("✅ STUN candidate gathering successful! Got both host and srflx candidates.");
}
