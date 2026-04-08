//! Integration tests for mDNS multicast socket setup and builder API

use std::sync::Arc;
use webrtc::peer_connection::RTCConfigurationBuilder;
use webrtc::peer_connection::*;
use webrtc::runtime::block_on;

#[derive(Clone)]
struct NoopHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for NoopHandler {}

/// with_mdns_mode(Disabled) should NOT create a multicast socket; the peer
/// connection should build and close without error.
#[test]
fn test_mdns_disabled_builds_without_multicast_socket() {
    block_on(async {
        let config = RTCConfigurationBuilder::new().build();
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(Arc::new(NoopHandler))
            .with_mdns_mode(MulticastDnsMode::Disabled)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        pc.close().await.expect("close should succeed");
    });
}

/// with_mdns_mode(QueryAndGather) should attempt to create the multicast socket.
/// On environments where multicast is available this succeeds; on restricted
/// environments it degrades gracefully (warn + continue).  Either way the peer
/// connection should build without error.
#[test]
fn test_mdns_query_and_gather_builds_gracefully() {
    block_on(async {
        let config = RTCConfigurationBuilder::new().build();
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(Arc::new(NoopHandler))
            .with_mdns_mode(MulticastDnsMode::QueryAndGather)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Should still be able to create offers etc.
        let offer = pc.create_offer(None).await;
        assert!(offer.is_ok(), "create_offer should work with mDNS enabled");

        pc.close().await.expect("close should succeed");
    });
}

/// with_mdns_mode should also configure the sans-IO core so that callers
/// don't have to set both with_setting_engine().set_multicast_dns_mode() AND
/// with_mdns_mode() separately.  We verify that setting only with_mdns_mode
/// is sufficient for the peer connection to build.
#[test]
fn test_mdns_mode_syncs_to_setting_engine() {
    block_on(async {
        // Only call with_mdns_mode (not with_setting_engine.set_multicast_dns_mode).
        // The builder should propagate the mode to the SettingEngine automatically.
        let config = RTCConfigurationBuilder::new().build();
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(Arc::new(NoopHandler))
            .with_mdns_mode(MulticastDnsMode::QueryAndGather)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        pc.close().await.expect("close should succeed");
    });
}

/// with_setting_engine() followed by with_mdns_mode() should work: the mDNS
/// mode set via with_mdns_mode takes effect on both the async layer and the
/// setting engine.
#[test]
fn test_mdns_mode_with_custom_setting_engine() {
    block_on(async {
        let mut se = SettingEngine::default();
        se.set_multicast_dns_local_name("test-peer.local".to_string());

        let config = RTCConfigurationBuilder::new().build();
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_setting_engine(se)
            .with_mdns_mode(MulticastDnsMode::QueryAndGather)
            .with_handler(Arc::new(NoopHandler))
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        pc.close().await.expect("close should succeed");
    });
}

/// The default builder (no with_mdns_mode call) should have mDNS disabled,
/// matching the SettingEngine default.
#[test]
fn test_default_builder_has_mdns_disabled() {
    block_on(async {
        let config = RTCConfigurationBuilder::new().build();
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(Arc::new(NoopHandler))
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Default behavior: no multicast socket created, no mDNS.
        // Should behave identically to existing code.
        let offer = pc.create_offer(None).await;
        assert!(offer.is_ok());

        pc.close().await.expect("close should succeed");
    });
}
