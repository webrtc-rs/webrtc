//! Integration tests for PeerConnection configuration options

use std::sync::Arc;
use std::time::Duration;
use webrtc::peer_connection::*;
use webrtc::runtime::block_on;
use webrtc::{
    MediaEngine, RTCBundlePolicy, RTCConfigurationBuilder, RTCIceServer, RTCIceTransportPolicy,
    RTCRtcpMuxPolicy, SettingEngine,
};

#[derive(Clone)]
struct ConfigTestHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for ConfigTestHandler {}

#[test]
fn test_media_engine_configuration() {
    block_on(async {
        // Test MediaEngine with default codecs
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register default codecs");

        let config = RTCConfigurationBuilder::new().build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        /*TODO:
        // Add a track first (needed to create offer with media)
        let track = rtc::media_stream::MediaStreamTrack::new(
            "stream".to_string(),
            "video".to_string(),
            "track".to_string(),
            rtc::rtp_transceiver::rtp_sender::RtpCodecKind::Video,
            vec![],
        );
        pc.add_track(track).await.expect("Failed to add track");*/

        // Create offer should work with registered codecs
        let offer = pc.create_offer(None).await.expect("Failed to create offer");
        assert!(!offer.sdp.is_empty(), "Offer SDP should not be empty");
    });
}

#[test]
fn test_setting_engine_ice_timeouts() {
    block_on(async {
        // Test SettingEngine with custom ICE timeouts
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let mut setting_engine = SettingEngine::default();

        // Configure ICE timeouts
        setting_engine.set_ice_timeouts(
            Some(Duration::from_secs(5)),  // disconnect timeout
            Some(Duration::from_secs(10)), // failed timeout
            Some(Duration::from_secs(1)),  // keepalive interval
        );

        let config = RTCConfigurationBuilder::new().build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_setting_engine(setting_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Should be able to create offer with custom settings
        let offer = pc.create_offer(None).await.expect("Failed to create offer");
        assert!(!offer.sdp.is_empty());
    });
}

#[test]
fn test_setting_engine_replay_protection() {
    block_on(async {
        // Test SettingEngine with custom replay protection windows
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let mut setting_engine = SettingEngine::default();

        // Configure replay protection window sizes
        setting_engine.set_srtp_replay_protection_window(128);
        setting_engine.set_srtcp_replay_protection_window(64);

        let config = RTCConfigurationBuilder::new().build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_setting_engine(setting_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        let offer = pc.create_offer(None).await.expect("Failed to create offer");
        assert!(!offer.sdp.is_empty());
    });
}

#[test]
fn test_combined_configuration() {
    block_on(async {
        // Test combining MediaEngine and SettingEngine configuration
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let mut setting_engine = SettingEngine::default();
        setting_engine.set_ice_timeouts(
            Some(Duration::from_secs(7)),
            Some(Duration::from_secs(15)),
            Some(Duration::from_secs(2)),
        );
        setting_engine.set_srtp_replay_protection_window(256);

        let config = RTCConfigurationBuilder::new().build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_setting_engine(setting_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Create and set local description
        let offer = pc.create_offer(None).await.expect("Failed to create offer");
        pc.set_local_description(offer.clone())
            .await
            .expect("Failed to set local description");

        // Verify local description was set
        let local_desc = pc
            .local_description()
            .await
            .expect("Local description should be set");
        assert_eq!(local_desc.sdp, offer.sdp);
    });
}

#[test]
fn test_peer_connection_with_full_configuration() {
    block_on(async {
        // End-to-end test with full configuration
        let mut media_engine_a = MediaEngine::default();
        media_engine_a
            .register_default_codecs()
            .expect("Failed to register codecs");
        let mut media_engine_b = MediaEngine::default();
        media_engine_b
            .register_default_codecs()
            .expect("Failed to register codecs");

        let mut setting_engine_a = SettingEngine::default();
        setting_engine_a.set_ice_timeouts(
            Some(Duration::from_secs(5)),
            Some(Duration::from_secs(10)),
            Some(Duration::from_secs(1)),
        );

        let mut setting_engine_b = SettingEngine::default();
        setting_engine_b.set_ice_timeouts(
            Some(Duration::from_secs(5)),
            Some(Duration::from_secs(10)),
            Some(Duration::from_secs(1)),
        );

        let config_a = RTCConfigurationBuilder::new().build();

        let config_b = RTCConfigurationBuilder::new().build();

        let handler_a = Arc::new(ConfigTestHandler);
        let handler_b = Arc::new(ConfigTestHandler);

        let pc_a = PeerConnectionBuilder::new()
            .with_configuration(config_a)
            .with_media_engine(media_engine_a)
            .with_setting_engine(setting_engine_a)
            .with_handler(handler_a)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();
        let pc_b = PeerConnectionBuilder::new()
            .with_configuration(config_b)
            .with_media_engine(media_engine_b)
            .with_setting_engine(setting_engine_b)
            .with_handler(handler_b)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        let _ = pc_a.create_data_channel("channel1", None).await.unwrap();

        // Perform offer/answer exchange
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

        // Both peers should have local and remote descriptions set
        assert!(pc_a.local_description().await.is_some());
        assert!(pc_a.remote_description().await.is_some());
        assert!(pc_b.local_description().await.is_some());
        assert!(pc_b.remote_description().await.is_some());
    });
}

#[test]
fn test_media_engine_required_for_tracks() {
    block_on(async {
        // Verify that MediaEngine with codecs is required for adding tracks
        let handler = Arc::new(ConfigTestHandler);

        // Create peer connection with MediaEngine
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let config = RTCConfigurationBuilder::new().build();

        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        /*TODO:
        // Add track
        let track = rtc::media_stream::MediaStreamTrack::new(
            "stream".to_string(),
            "video".to_string(),
            "track".to_string(),
            rtc::rtp_transceiver::rtp_sender::RtpCodecKind::Video,
            vec![],
        );

        // Should succeed
        let result = pc.add_track(track).await;
        assert!(
            result.is_ok(),
            "Adding track with MediaEngine should succeed"
        );*/

        // Creating offer should also succeed
        let offer_result = pc.create_offer(None);
        assert!(
            offer_result.await.is_ok(),
            "Creating offer with track should succeed"
        );
    });
}

#[test]
fn test_ice_servers_configuration() {
    block_on(async {
        // Test configuring ICE servers (STUN/TURN)
        let ice_servers = vec![
            RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                username: "".to_string(),
                credential: "".to_string(),
            },
            RTCIceServer {
                urls: vec!["turn:turn.example.com:3478".to_string()],
                username: "user".to_string(),
                credential: "pass".to_string(),
            },
        ];

        let config = RTCConfigurationBuilder::new()
            .with_ice_servers(ice_servers)
            .build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Should be able to create offers/answers with ICE servers configured
        let offer = pc.create_offer(None);
        assert!(offer.await.is_ok(), "Should create offer with ICE servers");
    });
}

#[test]
fn test_ice_transport_policy() {
    block_on(async {
        // Test ICE transport policy (all, relay-only, etc.)
        let config_all = RTCConfigurationBuilder::new()
            .with_ice_transport_policy(RTCIceTransportPolicy::All)
            .build();

        let config_relay = RTCConfigurationBuilder::new()
            .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
            .build();

        let handler_all = Arc::new(ConfigTestHandler);
        let handler_relay = Arc::new(ConfigTestHandler);

        let pc_all = PeerConnectionBuilder::new()
            .with_configuration(config_all)
            .with_handler(handler_all)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();
        let pc_relay = PeerConnectionBuilder::new()
            .with_configuration(config_relay)
            .with_handler(handler_relay)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Both should succeed
        assert!(pc_all.create_offer(None).await.is_ok());
        assert!(pc_relay.create_offer(None).await.is_ok());
    });
}

#[test]
fn test_bundle_policy() {
    block_on(async {
        // Test RTP bundle policy
        let config_balanced = RTCConfigurationBuilder::new()
            .with_bundle_policy(RTCBundlePolicy::Balanced)
            .build();

        let config_max_compat = RTCConfigurationBuilder::new()
            .with_bundle_policy(RTCBundlePolicy::MaxCompat)
            .build();

        let config_max_bundle = RTCConfigurationBuilder::new()
            .with_bundle_policy(RTCBundlePolicy::MaxBundle)
            .build();

        let handler = Arc::new(ConfigTestHandler);
        let pc_balanced = PeerConnectionBuilder::new()
            .with_configuration(config_balanced)
            .with_handler(handler.clone())
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();
        let pc_compat = PeerConnectionBuilder::new()
            .with_configuration(config_max_compat)
            .with_handler(handler.clone())
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();
        let pc_bundle = PeerConnectionBuilder::new()
            .with_configuration(config_max_bundle)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // All should create offers successfully
        assert!(pc_balanced.create_offer(None).await.is_ok());
        assert!(pc_compat.create_offer(None).await.is_ok());
        assert!(pc_bundle.create_offer(None).await.is_ok());
    });
}

#[test]
fn test_rtcp_mux_policy() {
    block_on(async {
        // Test RTCP multiplexing policy
        let config_require = RTCConfigurationBuilder::new()
            .with_rtcp_mux_policy(RTCRtcpMuxPolicy::Require)
            .build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config_require)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        let offer = pc.create_offer(None);
        assert!(offer.await.is_ok(), "Should create offer with RTCP mux");
    });
}

#[test]
fn test_peer_identity() {
    block_on(async {
        // Test peer identity configuration
        let config = RTCConfigurationBuilder::new()
            .with_peer_identitys("test-peer-identity".to_string())
            .build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        assert!(pc.create_offer(None).await.is_ok());
    });
}

#[test]
fn test_certificates() {
    block_on(async {
        // Test with custom certificates
        // Note: RTCCertificate::from_pem or generate would be used in real scenarios
        let certificates = vec![]; // Empty for now - real usage would load certs

        let config = RTCConfigurationBuilder::new()
            .with_certificates(certificates)
            .build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        assert!(pc.create_offer(None).await.is_ok());
    });
}

#[test]
fn test_ice_candidate_pool_size() {
    block_on(async {
        // Test ICE candidate pool size
        let config = RTCConfigurationBuilder::new()
            .with_ice_candidate_pool_size(5)
            .build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        assert!(pc.create_offer(None).await.is_ok());
    });
}

#[test]
fn test_all_configuration_options_combined() {
    block_on(async {
        // Test using all configuration options together
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let mut setting_engine = SettingEngine::default();
        setting_engine.set_ice_timeouts(
            Some(Duration::from_secs(5)),
            Some(Duration::from_secs(10)),
            Some(Duration::from_secs(1)),
        );

        let ice_servers = vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            username: "".to_string(),
            credential: "".to_string(),
        }];

        let config = RTCConfigurationBuilder::new()
            .with_ice_servers(ice_servers)
            .with_ice_transport_policy(RTCIceTransportPolicy::All)
            .with_bundle_policy(RTCBundlePolicy::MaxBundle)
            .with_rtcp_mux_policy(RTCRtcpMuxPolicy::Require)
            .with_peer_identitys("full-config-peer".to_string())
            .with_ice_candidate_pool_size(3)
            .build();

        let handler = Arc::new(ConfigTestHandler);
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_setting_engine(setting_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        /*TODO:
        // Add track
        let track = rtc::media_stream::MediaStreamTrack::new(
            "stream".to_string(),
            "video".to_string(),
            "track".to_string(),
            rtc::rtp_transceiver::rtp_sender::RtpCodecKind::Video,
            vec![],
        );
        pc.add_track(track).await.expect("Failed to add track");*/

        // Should create offer successfully with all options
        let offer = pc.create_offer(None).await.expect("Failed to create offer");
        assert!(!offer.sdp.is_empty(), "Offer should have SDP content");
    });
}
