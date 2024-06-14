use util::vnet::*;
use util::Conn;
use waitgroup::WaitGroup;

use super::agent_vnet_test::*;
use super::*;
use crate::agent::agent_transport::AgentConn;

pub(crate) async fn pipe(
    default_config0: Option<AgentConfig>,
    default_config1: Option<AgentConfig>,
) -> Result<(Arc<impl Conn>, Arc<impl Conn>, Arc<Agent>, Arc<Agent>)> {
    let (a_notifier, mut a_connected) = on_connected();
    let (b_notifier, mut b_connected) = on_connected();

    let mut cfg0 = default_config0.unwrap_or_default();
    cfg0.urls = vec![];
    cfg0.network_types = supported_network_types();

    let a_agent = Arc::new(Agent::new(cfg0).await?);
    a_agent.on_connection_state_change(a_notifier);

    let mut cfg1 = default_config1.unwrap_or_default();
    cfg1.urls = vec![];
    cfg1.network_types = supported_network_types();

    let b_agent = Arc::new(Agent::new(cfg1).await?);
    b_agent.on_connection_state_change(b_notifier);

    let (a_conn, b_conn) = connect_with_vnet(&a_agent, &b_agent).await?;

    // Ensure pair selected
    // Note: this assumes ConnectionStateConnected is thrown after selecting the final pair
    let _ = a_connected.recv().await;
    let _ = b_connected.recv().await;

    Ok((a_conn, b_conn, a_agent, b_agent))
}

#[tokio::test]
async fn test_remote_local_addr() -> Result<()> {
    // Agent0 is behind 1:1 NAT
    let nat_type0 = nat::NatType {
        mode: nat::NatMode::Nat1To1,
        ..Default::default()
    };
    // Agent1 is behind 1:1 NAT
    let nat_type1 = nat::NatType {
        mode: nat::NatMode::Nat1To1,
        ..Default::default()
    };

    let v = build_vnet(nat_type0, nat_type1).await?;

    let stun_server_url = Url {
        scheme: SchemeType::Stun,
        host: VNET_STUN_SERVER_IP.to_owned(),
        port: VNET_STUN_SERVER_PORT,
        proto: ProtoType::Udp,
        ..Default::default()
    };

    //"Disconnected Returns nil"
    {
        let disconnected_conn = AgentConn::new();
        let result = disconnected_conn.local_addr();
        assert!(result.is_err(), "Disconnected Returns nil");
    }

    //"Remote/Local Pair Match between Agents"
    {
        let (ca, cb) = pipe_with_vnet(
            &v,
            AgentTestConfig {
                urls: vec![stun_server_url.clone()],
                ..Default::default()
            },
            AgentTestConfig {
                urls: vec![stun_server_url],
                ..Default::default()
            },
        )
        .await?;

        let a_laddr = ca.local_addr()?;
        let b_laddr = cb.local_addr()?;

        // Assert addresses
        assert_eq!(a_laddr.ip().to_string(), VNET_LOCAL_IPA.to_string());
        assert_eq!(b_laddr.ip().to_string(), VNET_LOCAL_IPB.to_string());

        // Close
        //ca.close().await?;
        //cb.close().await?;
    }

    v.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_conn_stats() -> Result<()> {
    let (ca, cb, _, _) = pipe(None, None).await?;
    let na = ca.send(&[0u8; 10]).await?;

    let wg = WaitGroup::new();

    let w = wg.worker();
    tokio::spawn(async move {
        let _d = w;

        let mut buf = vec![0u8; 10];
        let nb = cb.recv(&mut buf).await?;
        assert_eq!(nb, 10, "bytes received don't match");

        Result::<()>::Ok(())
    });

    wg.wait().await;

    assert_eq!(na, 10, "bytes sent don't match");

    Ok(())
}
