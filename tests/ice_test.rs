//! Integration tests for ICE functionality

use rtc::ice::mdns::MulticastDnsMode;
use rtc::peer_connection::transport::RTCIceCandidate;
use rtc::stun::attributes::{ATTR_NONCE, ATTR_REALM};
use rtc::stun::error_code::CODE_UNAUTHORIZED;
use rtc::stun::message::{
    CLASS_ERROR_RESPONSE, CLASS_SUCCESS_RESPONSE, METHOD_ALLOCATE, METHOD_CREATE_PERMISSION,
    Message as StunMessage, MessageType,
};
use rtc::stun::textattrs::{Nonce, Realm};
use rtc::turn::proto::lifetime::Lifetime;
use rtc::turn::proto::relayaddr::RelayedAddress;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use webrtc::peer_connection::*;
use webrtc::peer_connection::{
    MediaEngine, RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceCandidateType,
    RTCIceGatheringState, RTCIceServer, RTCIceTransportPolicy, RTCPeerConnectionIceEvent,
    RTCPeerConnectionState,
};
use webrtc::runtime::{AsyncUdpSocket, default_runtime, timeout};
use webrtc::runtime::{Mutex, Sender};
use webrtc::runtime::{block_on, channel};

#[derive(Clone)]
struct IceTestHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for IceTestHandler {}

#[derive(Clone)]
struct IceGatheringHandler {
    candidate_tx: Sender<RTCIceCandidate>,
    gathering_tx: Sender<()>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for IceGatheringHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        eprintln!("✅ Received ICE candidate event: {:?}", event.candidate);
        let _ = self.candidate_tx.try_send(event.candidate);
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        eprintln!("ICE gathering state: {:?}", state);
        if state == RTCIceGatheringState::Complete {
            let _ = self.gathering_tx.try_send(());
        }
    }
}

// Handler that tracks candidate types for STUN testing
struct CandidateTypeTracker {
    candidates: Arc<Mutex<Vec<RTCIceCandidateType>>>,
    gathering_tx: Sender<()>,
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

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        eprintln!("ICE gathering state: {:?}", state);
        if state == RTCIceGatheringState::Complete {
            let _ = self.gathering_tx.try_send(());
        }
    }
}

async fn run_mock_turn_server(turn_socket: Arc<dyn AsyncUdpSocket>, relay_addr: SocketAddr) {
    let mut buf = vec![0u8; 2048];
    loop {
        let Ok((n, peer_addr)) = turn_socket.recv_from(&mut buf).await else {
            break;
        };

        let mut msg = StunMessage::new();
        msg.raw = buf[..n].to_vec();
        if msg.decode().is_err() {
            continue;
        }

        let response = match msg.typ.method {
            METHOD_ALLOCATE => {
                if msg.get(ATTR_NONCE).is_ok() {
                    build_turn_allocate_success(msg.transaction_id, relay_addr)
                } else {
                    build_turn_allocate_unauthorized(msg.transaction_id)
                }
            }
            METHOD_CREATE_PERMISSION => build_turn_create_permission_success(msg.transaction_id),
            _ => continue,
        };

        if turn_socket.send_to(&response.raw, peer_addr).await.is_err() {
            break;
        }
    }
}

fn build_turn_allocate_unauthorized(
    transaction_id: rtc::stun::message::TransactionId,
) -> StunMessage {
    let mut msg = StunMessage::new();
    msg.build(&[
        Box::new(transaction_id),
        Box::new(MessageType::new(METHOD_ALLOCATE, CLASS_ERROR_RESPONSE)),
        Box::new(CODE_UNAUTHORIZED),
        Box::new(Realm::new(ATTR_REALM, "webrtc.rs".to_owned())),
        Box::new(Nonce::new(ATTR_NONCE, "nonce".to_owned())),
    ])
    .expect("failed to build TURN unauthorized response");
    msg
}

fn build_turn_allocate_success(
    transaction_id: rtc::stun::message::TransactionId,
    relay_addr: SocketAddr,
) -> StunMessage {
    let mut msg = StunMessage::new();
    msg.build(&[
        Box::new(transaction_id),
        Box::new(MessageType::new(METHOD_ALLOCATE, CLASS_SUCCESS_RESPONSE)),
        Box::new(RelayedAddress {
            ip: relay_addr.ip(),
            port: relay_addr.port(),
        }),
        Box::new(Lifetime(Duration::from_secs(600))),
    ])
    .expect("failed to build TURN allocate success response");
    msg
}

fn build_turn_create_permission_success(
    transaction_id: rtc::stun::message::TransactionId,
) -> StunMessage {
    let mut msg = StunMessage::new();
    msg.build(&[
        Box::new(transaction_id),
        Box::new(MessageType::new(
            METHOD_CREATE_PERMISSION,
            CLASS_SUCCESS_RESPONSE,
        )),
    ])
    .expect("failed to build TURN create-permission success response");
    msg
}

#[test]
fn test_add_ice_candidate() {
    block_on(async {
        // Test adding remote ICE candidates
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let config = RTCConfigurationBuilder::new().build();

        let handler = Arc::new(IceTestHandler);

        let pc_a = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_handler(handler.clone())
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        let mut media_engine_b = MediaEngine::default();
        media_engine_b
            .register_default_codecs()
            .expect("Failed to register codecs");
        let config_b = RTCConfigurationBuilder::new().build();

        let pc_b = PeerConnectionBuilder::new()
            .with_configuration(config_b)
            .with_media_engine(media_engine_b)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        let _ = pc_a.create_data_channel("channel1", None).await.unwrap();

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
    });
}

#[test]
fn test_restart_ice() {
    block_on(async {
        // Test ICE restart API
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let config = RTCConfigurationBuilder::new().build();

        let handler = Arc::new(IceTestHandler);

        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

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
    });
}

#[test]
fn test_automatic_host_candidate_gathering() {
    block_on(async {
        env_logger::builder()
            .filter_level(log::LevelFilter::Trace)
            //.is_test(true)
            .try_init()
            .ok();

        // Test that host candidates are automatically gathered when setLocalDescription is called
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let config = RTCConfigurationBuilder::new().build();

        let (candidate_tx, mut candidate_rx) = channel(32);
        let (gathering_tx, mut gathering_rx) = channel(8);
        let handler = Arc::new(IceGatheringHandler {
            candidate_tx,
            gathering_tx,
        });

        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["0.0.0.0:0"])
            .build()
            .await
            .unwrap();

        let _ = pc.create_data_channel("channel1", None).await.unwrap();

        // Create and set local description - this should trigger gathering
        let offer = pc.create_offer(None).await.expect("Failed to create offer");
        pc.set_local_description(offer)
            .await
            .expect("Failed to set local description");

        // Give the driver time to process events
        let _ = gathering_rx.recv().await;

        // Verify that a host candidate was gathered
        let mut candidate_count = 0;
        while let Some(_) = candidate_rx.recv().await {
            candidate_count += 1;
            break;
        }
        assert!(
            candidate_count > 0,
            "Should have received at least one ICE candidate"
        );

        println!("✅ Host candidate gathering successful!");
    });
}

#[test]
fn test_stun_gathering_with_google_stun() {
    block_on(async {
        env_logger::builder()
            .filter_level(log::LevelFilter::Trace)
            //.is_test(true)
            .try_init()
            .ok();

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
            .with_ice_servers(ice_servers)
            .build();

        // Track candidate types to verify we get both host and srflx
        let candidates = Arc::new(Mutex::new(Vec::new()));
        let (gathering_tx, mut gathering_rx) = channel(8);
        let handler = Arc::new(CandidateTypeTracker {
            candidates: candidates.clone(),
            gathering_tx,
        });

        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["0.0.0.0:0", "[::]:0"])
            .build()
            .await
            .unwrap();

        let _ = pc.create_data_channel("channel1", None).await.unwrap();

        // Create and set local description - this should trigger gathering
        let offer = pc.create_offer(None).await.expect("Failed to create offer");
        pc.set_local_description(offer)
            .await
            .expect("Failed to set local description");

        // Wait for both host and STUN candidates to arrive
        // We expect at least: 1 host + 1 srflx = 2 candidates
        println!("⏳ Waiting for ICE candidates...");
        let _ = gathering_rx.recv().await;

        println!("⏳ ICE Gathering Completed!...");

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
    });
}

#[test]
fn test_mdns_query_and_gather_rewrites_host_candidate() {
    block_on(async {
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let config = RTCConfigurationBuilder::new().build();

        let (candidate_tx, mut candidate_rx) = channel(32);
        let (gathering_tx, mut gathering_rx) = channel(8);
        let handler = Arc::new(IceGatheringHandler {
            candidate_tx,
            gathering_tx,
        });

        let mut setting_engine = SettingEngine::default();
        setting_engine.set_multicast_dns_mode(MulticastDnsMode::QueryAndGather);
        setting_engine.set_multicast_dns_timeout(Some(std::time::Duration::from_secs(5)));
        setting_engine.set_multicast_dns_local_name("async-mdns-host.local".to_owned());
        setting_engine.set_multicast_dns_local_ip(Some(std::net::Ipv4Addr::LOCALHOST.into()));

        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_setting_engine(setting_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        let _ = pc.create_data_channel("channel1", None).await.unwrap();

        let offer = pc.create_offer(None).await.expect("Failed to create offer");
        pc.set_local_description(offer)
            .await
            .expect("Failed to set local description");

        let _ = gathering_rx.recv().await;

        let mut found_mdns_host_candidate = false;
        while let Some(candidate) = candidate_rx.recv().await {
            if candidate.typ == RTCIceCandidateType::Host {
                assert!(
                    candidate.address.ends_with(".local"),
                    "expected mDNS-obfuscated host candidate, got {}",
                    candidate.address
                );
                found_mdns_host_candidate = true;
                break;
            }
        }

        assert!(
            found_mdns_host_candidate,
            "Should have received at least one mDNS host candidate"
        );
    });
}

#[test]
fn test_turn_relay_gathering_with_mock_turn_server() {
    block_on(async {
        let runtime = default_runtime().expect("no async runtime available");
        let turn_socket =
            std::net::UdpSocket::bind("127.0.0.1:0").expect("failed to bind mock TURN server");
        let turn_addr = turn_socket
            .local_addr()
            .expect("failed to get mock TURN address");
        let turn_socket = runtime
            .wrap_udp_socket(turn_socket)
            .expect("failed to wrap mock TURN socket");
        let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
        let turn_task = runtime.spawn(Box::pin(run_mock_turn_server(turn_socket, relay_addr)));

        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .expect("Failed to register codecs");

        let ice_servers = vec![RTCIceServer {
            urls: vec![format!("turn:{}?transport=udp", turn_addr)],
            username: "user".to_owned(),
            credential: "pass".to_owned(),
        }];

        let config = RTCConfigurationBuilder::new()
            .with_ice_servers(ice_servers)
            .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
            .build();

        let candidates = Arc::new(Mutex::new(Vec::new()));
        let (gathering_tx, mut gathering_rx) = channel(8);
        let handler = Arc::new(CandidateTypeTracker {
            candidates: candidates.clone(),
            gathering_tx,
        });

        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_media_engine(media_engine)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        let _ = pc.create_data_channel("channel1", None).await.unwrap();

        let offer = pc.create_offer(None).await.expect("Failed to create offer");
        pc.set_local_description(offer)
            .await
            .expect("Failed to set local description");

        timeout(Duration::from_secs(5), gathering_rx.recv())
            .await
            .expect("Timed out waiting for relay gathering to complete");

        let gathered: Vec<RTCIceCandidateType> = candidates.lock().await.clone();
        assert!(
            gathered.contains(&RTCIceCandidateType::Relay),
            "Expected a relay candidate, got {:?}",
            gathered
        );
        assert!(
            !gathered.contains(&RTCIceCandidateType::Host),
            "Relay-only policy should not publish host candidates: {:?}",
            gathered
        );

        turn_task.abort();
    });
}

#[test]
fn test_ice_tcp_only_connection() {
    block_on(async {
        env_logger::builder()
            .filter_level(log::LevelFilter::Trace)
            .is_test(true)
            .try_init()
            .ok();

        let runtime = default_runtime().expect("no async runtime found");

        let (a_candidate_tx, mut a_candidate_rx) = channel::<RTCIceCandidateInit>(32);
        let (b_candidate_tx, mut b_candidate_rx) = channel::<RTCIceCandidateInit>(32);

        let (a_connected_tx, mut a_connected_rx) = channel::<()>(1);
        let (b_connected_tx, mut b_connected_rx) = channel::<()>(1);

        struct TestHandler {
            candidate_tx: Sender<RTCIceCandidateInit>,
            connected_tx: Sender<()>,
        }

        #[async_trait::async_trait]
        impl PeerConnectionEventHandler for TestHandler {
            async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
                if let Ok(cand_init) = event.candidate.to_json() {
                    if !cand_init.candidate.is_empty() {
                        let _ = self.candidate_tx.try_send(cand_init);
                    }
                }
            }

            async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
                if state == RTCPeerConnectionState::Connected {
                    let _ = self.connected_tx.try_send(());
                }
            }
        }

        let mut media_engine_a = MediaEngine::default();
        media_engine_a.register_default_codecs().unwrap();
        let pc_a = PeerConnectionBuilder::new()
            .with_media_engine(media_engine_a)
            .with_handler(Arc::new(TestHandler {
                candidate_tx: a_candidate_tx,
                connected_tx: a_connected_tx,
            }))
            .with_tcp_addrs(vec!["127.0.0.1:0"])
            .with_udp_addrs(Vec::<&str>::new()) // Force TCP only
            .build()
            .await
            .unwrap();
        let pc_a = Arc::new(pc_a);

        let mut media_engine_b = MediaEngine::default();
        media_engine_b.register_default_codecs().unwrap();
        let pc_b = PeerConnectionBuilder::new()
            .with_media_engine(media_engine_b)
            .with_handler(Arc::new(TestHandler {
                candidate_tx: b_candidate_tx,
                connected_tx: b_connected_tx,
            }))
            .with_tcp_addrs(vec!["127.0.0.1:0"])
            .with_udp_addrs(Vec::<&str>::new()) // Force TCP only
            .build()
            .await
            .unwrap();
        let pc_b = Arc::new(pc_b);

        // Create data channel to ensure DTLS/SCTP handshakes happen
        let _dc_a = pc_a.create_data_channel("test-tcp", None).await.unwrap();

        let offer = pc_a.create_offer(None).await.unwrap();
        pc_a.set_local_description(offer.clone()).await.unwrap();
        pc_b.set_remote_description(offer).await.unwrap();

        let answer = pc_b.create_answer(None).await.unwrap();
        pc_b.set_local_description(answer.clone()).await.unwrap();
        pc_a.set_remote_description(answer).await.unwrap();

        // Relay candidates in background tasks
        let pc_a_clone = pc_a.clone();
        let pc_b_clone = pc_b.clone();

        let task_a = runtime.spawn(Box::pin(async move {
            while let Some(cand) = a_candidate_rx.recv().await {
                let _ = pc_b_clone.add_ice_candidate(cand).await;
            }
        }));

        let task_b = runtime.spawn(Box::pin(async move {
            while let Some(cand) = b_candidate_rx.recv().await {
                let _ = pc_a_clone.add_ice_candidate(cand).await;
            }
        }));

        // Wait for connection
        timeout(Duration::from_secs(10), async {
            let _ = a_connected_rx.recv().await;
            let _ = b_connected_rx.recv().await;
        })
        .await
        .expect("Timed out waiting for TCP PeerConnection connection to establish");

        task_a.abort();
        task_b.abort();
    });
}
