//! Integration tests for track APIs

use bytes::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use webrtc::media_track::TrackRemote;
use webrtc::peer_connection::PeerConnection;
use webrtc::peer_connection_event_handler::PeerConnectionEventHandler;
use webrtc::runtime::Mutex;
use webrtc::runtime::sleep;
use webrtc::{MediaEngine, RTCConfigurationBuilder};

struct TrackTestHandler {
    remote_track: Arc<Mutex<Option<Arc<TrackRemote>>>>,
}

impl TrackTestHandler {
    fn new() -> Self {
        Self {
            remote_track: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for TrackTestHandler {
    async fn on_track_open(&self, track: Arc<TrackRemote>) {
        println!("Track opened");
        *self.remote_track.lock().await = Some(track);
    }
}

/// Helper to create a video track for testing
fn create_video_track() -> rtc::media_stream::MediaStreamTrack {
    use rtc::rtp_transceiver::rtp_sender::RtpCodecKind;

    rtc::media_stream::MediaStreamTrack::new(
        "test-stream".to_string(),
        "video".to_string(),
        "test-video-track".to_string(),
        RtpCodecKind::Video,
        vec![], // Encodings added during negotiation
    )
}

#[tokio::test]
async fn test_add_track() {
    let config = RTCConfigurationBuilder::new().build();
    let handler = Arc::new(TrackTestHandler::new());

    let pc = PeerConnection::new(config, handler).expect("Failed to create peer connection");

    // Create and add a track
    let track = create_video_track();
    let local_track = pc.add_track(track).await.expect("Failed to add track");

    // Verify we got a TrackLocal back (can use it to send)
    let _ = local_track;
}

#[tokio::test]
async fn test_send_rtp_packets() {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("Failed to register codecs");

    let config = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine)
        .build();
    let handler = Arc::new(TrackTestHandler::new());

    let pc = PeerConnection::new(config, handler).expect("Failed to create peer connection");

    // Add a track
    let track = create_video_track();
    let local_track = pc.add_track(track).await.expect("Failed to add track");

    // Create offer to initialize negotiation
    let _offer = pc.create_offer(None).await.expect("Failed to create offer");

    // Create and send RTP packets
    for seq in 1000..1010 {
        let packet = rtc::rtp::packet::Packet {
            header: rtc::rtp::header::Header {
                version: 2,
                padding: false,
                extension: false,
                marker: seq == 1000,
                payload_type: 96,
                sequence_number: seq,
                timestamp: (seq as u32) * 3000,
                ssrc: 12345,
                csrc: vec![],
                extension_profile: 0,
                extensions: vec![],
                extensions_padding: 0,
            },
            payload: Bytes::from(vec![0xAA; 100]),
        };

        // Send RTP packet - should queue without error
        local_track
            .write_rtp(packet)
            .await
            .expect("Failed to send RTP");
    }
}

// Test sending RTCP packets via TrackLocal
#[tokio::test]
async fn test_send_rtcp_packets() {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("Failed to register codecs");

    let config = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine)
        .build();
    let handler = Arc::new(TrackTestHandler::new());

    let pc = PeerConnection::new(config, handler).expect("Failed to create peer connection");

    // Add a track
    let track = create_video_track();
    let local_track = pc.add_track(track).await.expect("Failed to add track");

    // Create RTCP sender report
    let sr = rtc::rtcp::sender_report::SenderReport {
        ssrc: 12345,
        ntp_time: 0x123456789ABCDEF0,
        rtp_time: 48000,
        packet_count: 100,
        octet_count: 10000,
        reports: vec![],
        profile_extensions: Bytes::new(),
    };

    // Send RTCP packets
    let packets: Vec<Box<dyn rtc::rtcp::Packet>> = vec![Box::new(sr)];
    local_track
        .write_rtcp(packets)
        .await
        .expect("Failed to send RTCP");
}

// End-to-end track negotiation test
#[tokio::test]
async fn test_track_negotiation() {
    // Create two peer connections with media engine
    let mut media_engine_a = MediaEngine::default();
    media_engine_a
        .register_default_codecs()
        .expect("Failed to register codecs");
    let mut media_engine_b = MediaEngine::default();
    media_engine_b
        .register_default_codecs()
        .expect("Failed to register codecs");

    let config_a = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine_a)
        .build();
    let config_b = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine_b)
        .build();

    let handler_a = Arc::new(TrackTestHandler::new());
    let handler_b = Arc::new(TrackTestHandler::new());

    let pc_a = PeerConnection::new(config_a, handler_a.clone())
        .expect("Failed to create peer connection A");
    let pc_b = PeerConnection::new(config_b, handler_b.clone())
        .expect("Failed to create peer connection B");

    // Add track to peer A
    let track = create_video_track();
    let local_track = pc_a.add_track(track).await.expect("Failed to add track");

    // Create offer/answer exchange
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

    // Bind and start drivers (in background)
    let addr_a: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let addr_b: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let driver_a = pc_a.bind(addr_a).await.expect("Failed to bind A");
    let driver_b = pc_b.bind(addr_b).await.expect("Failed to bind B");

    let handle_a = tokio::spawn(async move {
        let _ = driver_a.run().await;
    });

    let handle_b = tokio::spawn(async move {
        let _ = driver_b.run().await;
    });

    // Wait for track to be negotiated on peer B
    sleep(Duration::from_millis(200)).await;

    // Send some RTP packets from A
    for seq in 2000..2005 {
        let packet = rtc::rtp::packet::Packet {
            header: rtc::rtp::header::Header {
                version: 2,
                padding: false,
                extension: false,
                marker: seq == 2000,
                payload_type: 96,
                sequence_number: seq,
                timestamp: (seq as u32) * 3000,
                ssrc: 54321,
                csrc: vec![],
                extension_profile: 0,
                extensions: vec![],
                extensions_padding: 0,
            },
            payload: Bytes::from(vec![0xBB; 150]),
        };

        local_track
            .write_rtp(packet)
            .await
            .expect("Failed to send RTP");
    }

    // Give time for cleanup
    sleep(Duration::from_millis(100)).await;

    // Cleanup
    handle_a.abort();
    handle_b.abort();
}

// Test sending RTCP feedback from TrackRemote
#[tokio::test]
async fn test_send_rtcp_feedback() {
    let mut media_engine_a = MediaEngine::default();
    media_engine_a
        .register_default_codecs()
        .expect("Failed to register codecs");
    let mut media_engine_b = MediaEngine::default();
    media_engine_b
        .register_default_codecs()
        .expect("Failed to register codecs");

    let config_a = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine_a)
        .build();
    let config_b = RTCConfigurationBuilder::new()
        .with_media_engine(media_engine_b)
        .build();

    let handler_a = Arc::new(TrackTestHandler::new());
    let handler_b = Arc::new(TrackTestHandler::new());

    let pc_a = PeerConnection::new(config_a, handler_a.clone())
        .expect("Failed to create peer connection A");
    let pc_b = PeerConnection::new(config_b, handler_b.clone())
        .expect("Failed to create peer connection B");

    // Add track to peer B (A will be the receiver)
    let track = create_video_track();
    let _local_track = pc_b.add_track(track).await.expect("Failed to add track");

    // Create offer/answer
    let offer = pc_b
        .create_offer(None)
        .await
        .expect("Failed to create offer");
    pc_b.set_local_description(offer.clone())
        .await
        .expect("Failed to set local description");
    pc_a.set_remote_description(offer)
        .await
        .expect("Failed to set remote description");

    let answer = pc_a
        .create_answer(None)
        .await
        .expect("Failed to create answer");
    pc_a.set_local_description(answer.clone())
        .await
        .expect("Failed to set local description");
    pc_b.set_remote_description(answer)
        .await
        .expect("Failed to set remote description");

    // Bind peers
    let addr_a: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let addr_b: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let driver_a = pc_a.bind(addr_a).await.expect("Failed to bind A");
    let driver_b = pc_b.bind(addr_b).await.expect("Failed to bind B");

    let handle_a = tokio::spawn(async move {
        let _ = driver_a.run().await;
    });

    let handle_b = tokio::spawn(async move {
        let _ = driver_b.run().await;
    });

    // Wait for negotiation
    sleep(Duration::from_millis(200)).await;

    // Check if we got a remote track and send feedback
    if let Some(remote_track) = handler_a.remote_track.lock().await.as_ref() {
        // Send RTCP feedback (Receiver Report)
        let rr = rtc::rtcp::receiver_report::ReceiverReport {
            ssrc: 99999,
            reports: vec![],
            profile_extensions: Bytes::new(),
        };

        let packets: Vec<Box<dyn rtc::rtcp::Packet>> = vec![Box::new(rr)];
        remote_track
            .write_rtcp(packets)
            .await
            .expect("Failed to send RTCP feedback");
    }

    // Cleanup
    sleep(Duration::from_millis(100)).await;
    handle_a.abort();
    handle_b.abort();
}
