use std::time::Duration;

use tokio::net::UdpSocket;

use super::candidate_relay_test::OptimisticAuthHandler;
use super::*;
use crate::agent::agent_config::AgentConfig;
use crate::agent::agent_vnet_test::{connect_with_vnet, on_connected};
use crate::agent::Agent;
use crate::url::{SchemeType, Url};

//use std::io::Write;

#[tokio::test]
async fn test_server_reflexive_only_connection() -> Result<()> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, log::LevelFilter::Trace)
    .init();*/

    let server_listener = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let server_port = server_listener.local_addr()?.port();

    let server = turn::server::Server::new(turn::server::config::ServerConfig {
        realm: "webrtc.rs".to_owned(),
        auth_handler: Arc::new(OptimisticAuthHandler {}),
        conn_configs: vec![turn::server::config::ConnConfig {
            conn: server_listener,
            relay_addr_generator: Box::new(turn::relay::relay_none::RelayAddressGeneratorNone {
                address: "127.0.0.1".to_owned(),
                net: Arc::new(util::vnet::net::Net::new(None)),
            }),
        }],
        channel_bind_timeout: Duration::from_secs(0),
        alloc_close_notify: None,
    })
    .await?;

    let cfg0 = AgentConfig {
        network_types: vec![NetworkType::Udp4],
        urls: vec![Url {
            scheme: SchemeType::Stun,
            host: "127.0.0.1".to_owned(),
            port: server_port,
            ..Default::default()
        }],
        candidate_types: vec![CandidateType::ServerReflexive],
        ..Default::default()
    };

    let a_agent = Arc::new(Agent::new(cfg0).await?);
    let (a_notifier, mut a_connected) = on_connected();
    a_agent.on_connection_state_change(a_notifier);

    let cfg1 = AgentConfig {
        network_types: vec![NetworkType::Udp4],
        urls: vec![Url {
            scheme: SchemeType::Stun,
            host: "127.0.0.1".to_owned(),
            port: server_port,
            ..Default::default()
        }],
        candidate_types: vec![CandidateType::ServerReflexive],
        ..Default::default()
    };

    let b_agent = Arc::new(Agent::new(cfg1).await?);
    let (b_notifier, mut b_connected) = on_connected();
    b_agent.on_connection_state_change(b_notifier);

    connect_with_vnet(&a_agent, &b_agent).await?;

    let _ = a_connected.recv().await;
    let _ = b_connected.recv().await;

    a_agent.close().await?;
    b_agent.close().await?;
    server.close().await?;

    Ok(())
}
