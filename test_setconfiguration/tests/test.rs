// tests/test.rs
use webrtc::api::API;
//use webrtc::ice_transport::ice_server::RTCIceServer;
//use webrtc::peer_connection::configuration::RTCConfiguration;
//use webrtc::peer_connection::RTCPeerConnection;
//use webrtc::webrtc::RTCIceCredentialType;
use webrtc::{
    api::{interceptor_registry::register_default_interceptors, media_engine::*, APIBuilder},
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        RTCPeerConnection,
    },
    rtp_transceiver::{
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters},
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
        RTCRtpTransceiverInit,
    },
    util::MarshalSize,
};

use webrtc::peer_connection::policy::rtcp_mux_policy::RTCRtcpMuxPolicy;
use webrtc::peer_connection::policy::bundle_policy::RTCBundlePolicy;
use webrtc::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy;
use webrtc::ice_transport::ice_credential_type::RTCIceCredentialType;
use std::sync::Arc;
use tokio::sync::Mutex;



#[tokio::test]
async fn test_set_get_configuration() {
    
    
    // 初始化 MediaEngine 和 InterceptorRegistry
    let mut media_engine = MediaEngine::default();
    let mut registry = Registry::new();

    // 注册默认拦截器
    register_default_interceptors( registry, &mut media_engine).expect("Failed to register default interceptors");


    // 初始化 MediaEngine 和 InterceptorRegistry
    //let mut media_engine = MediaEngine::default();
    //register_default_interceptors(&mut media_engine).expect("Failed to register default interceptors");

    let registry = Registry::default();

    // 创建 API 实例
    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    // 创建初始配置
    let initial_config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            username: "".to_string(),
            credential: "".to_string(),
            credential_type: RTCIceCredentialType::Unspecified,
        }],
        ..Default::default()
    };

    // 创建 RTCPeerConnection 实例
    let peer = Arc::new(
        api.new_peer_connection(initial_config.clone())
            .await
            .expect("Failed to create RTCPeerConnection"),
    );

    // 获取并打印初始配置
    let config_before = peer.get_configuration().await;
    /*
    println!("Initial ICE Servers: {:?}", config_before.ice_servers);
    println!("Initial ICE Transport Policy: {:?}", config_before.ice_transport_policy);
    println!("Initial Bundle Policy: {:?}", config_before.bundle_policy);
    println!("Initial RTCP Mux Policy: {:?}", config_before.rtcp_mux_policy);
    println!("Initial Peer Identity: {:?}", config_before.peer_identity);
    println!("Initial Certificates: {:?}", config_before.certificates);
    println!("Initial ICE Candidate Pool Size: {:?}", config_before.ice_candidate_pool_size);
    */
    // 创建新配置
    let new_config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["turn:turn.22333.fun".to_string(), "turn:cn.22333.fun".to_string()],
            username: "live777".to_string(),
            credential: "live777".to_string(),
            credential_type: RTCIceCredentialType::Password,
        }],
        ..Default::default()
    };

    // 设置新配置
    peer.set_configuration(new_config.clone())
        .await
        .expect("Failed to set configuration");

    // 获取并打印更新后的配置
    let updated_config = peer.get_configuration().await;
    //println!("Updated ICE Servers: {:?}", updated_config.ice_servers);
    //println!("Updated ICE Transport Policy: {:?}", updated_config.ice_transport_policy);
    //println!("Updated Bundle Policy: {:?}", updated_config.bundle_policy);
    //println!("Updated RTCP Mux Policy: {:?}", updated_config.rtcp_mux_policy);
    //println!("Updated Peer Identity: {:?}", updated_config.peer_identity);
    //println!("Updated Certificates: {:?}", updated_config.certificates);
    //println!("Updated ICE Candidate Pool Size: {:?}", updated_config.ice_candidate_pool_size);
        // Assertions for updated configuration
    
    assert_eq!(updated_config.ice_servers.len(), 1);
    assert_eq!(updated_config.ice_servers[0].urls, vec!["turn:turn.22333.fun".to_string(), "turn:cn.22333.fun".to_string()]);
    assert_eq!(updated_config.ice_servers[0].username, "live777");
    assert_eq!(updated_config.ice_servers[0].credential, "live777");
    assert_eq!(updated_config.ice_servers[0].credential_type, RTCIceCredentialType::Password);
    assert_eq!(updated_config.ice_transport_policy, RTCIceTransportPolicy::Unspecified);
    assert_eq!(updated_config.bundle_policy, RTCBundlePolicy::Unspecified);
    assert_eq!(updated_config.rtcp_mux_policy, RTCRtcpMuxPolicy::Unspecified);
    assert!(updated_config.peer_identity.is_empty());
    assert!(updated_config.certificates.is_empty());
    assert_eq!(updated_config.ice_candidate_pool_size, 0);
    
    // 验证配置是否已更新
    //assert_eq!(updated_config.ice_servers, new_config.ice_servers);
}

