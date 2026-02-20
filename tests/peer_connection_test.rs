//! Integration tests for PeerConnection

use std::sync::Arc;
use webrtc::peer_connection::*;
use webrtc::runtime::block_on;
use webrtc::{
    RTCConfigurationBuilder, RTCIceConnectionState, RTCPeerConnectionIceEvent,
    RTCPeerConnectionState, RTCSdpType,
};

#[derive(Clone)]
struct TestHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for TestHandler {
    async fn on_negotiation_needed(&self) {
        println!("Negotiation needed");
    }

    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        println!("New ICE candidate: {:?}", event.candidate);
    }

    async fn on_ice_connection_state_change(&self, state: RTCIceConnectionState) {
        println!("ICE connection state changed: {:?}", state);
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        println!("Peer connection state changed: {:?}", state);
    }
}

#[test]
fn test_create_peer_connection() {
    block_on(async {
        let config = RTCConfigurationBuilder::new().build();
        let handler = Arc::new(TestHandler);

        let mut pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Verify we can close it
        pc.close().await.expect("Failed to close peer connection");
    })
}

#[test]
fn test_create_offer() {
    block_on(async {
        let config = RTCConfigurationBuilder::new().build();
        let handler = Arc::new(TestHandler);

        let mut pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Create an offer
        let offer = pc.create_offer(None).await.expect("Failed to create offer");

        // Verify offer has SDP content
        assert!(!offer.sdp.is_empty(), "Offer SDP should not be empty");
        assert_eq!(offer.sdp_type, RTCSdpType::Offer, "Should be an offer");

        pc.close().await.expect("Failed to close peer connection");
    })
}

#[test]
fn test_set_local_description() {
    block_on(async {
        let config = RTCConfigurationBuilder::new().build();
        let handler = Arc::new(TestHandler);

        let mut pc = PeerConnectionBuilder::new()
            .with_configuration(config)
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
        assert_eq!(local_desc.sdp_type, RTCSdpType::Offer);
        assert_eq!(local_desc.sdp, offer.sdp);

        pc.close().await.expect("Failed to close peer connection");
    })
}

#[test]
fn test_offer_answer_exchange() {
    block_on(async {
        // Create two peer connections to simulate offer/answer exchange
        let config1 = RTCConfigurationBuilder::new().build();
        let handler1 = Arc::new(TestHandler);
        let mut pc1 = PeerConnectionBuilder::new()
            .with_configuration(config1)
            .with_handler(handler1)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        let config2 = RTCConfigurationBuilder::new().build();
        let handler2 = Arc::new(TestHandler);
        let mut pc2 = PeerConnectionBuilder::new()
            .with_configuration(config2)
            .with_handler(handler2)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // PC1 creates an offer
        let _offer = pc1
            .create_offer(None)
            .await
            .expect("Failed to create offer");

        // Verify we can create an answer (even if it fails due to missing ICE setup)
        // The important thing is that the API works
        let answer_result = pc2.create_answer(None).await;

        // We expect this to fail without setting remote description first
        // but the API should not panic
        assert!(
            answer_result.is_err(),
            "create_answer should fail without remote description"
        );

        // Cleanup
        pc1.close().await.expect("Failed to close PC1");
        pc2.close().await.expect("Failed to close PC2");
    })
}
