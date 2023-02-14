use super::*;

use crate::candidate::candidate_base::unmarshal_candidate;
use async_trait::async_trait;
use std::net::{IpAddr, Ipv4Addr};
use std::result::Result;
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use util::vnet::chunk::Chunk;
use util::{vnet::router::Nic, vnet::*, Conn};
use waitgroup::WaitGroup;

pub(crate) struct MockConn;

#[async_trait]
impl Conn for MockConn {
    async fn connect(&self, _addr: SocketAddr) -> Result<(), util::Error> {
        Ok(())
    }
    async fn recv(&self, _buf: &mut [u8]) -> Result<usize, util::Error> {
        Ok(0)
    }
    async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr), util::Error> {
        Ok((0, SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0)))
    }
    async fn send(&self, _buf: &[u8]) -> Result<usize, util::Error> {
        Ok(0)
    }
    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> Result<usize, util::Error> {
        Ok(0)
    }
    fn local_addr(&self) -> Result<SocketAddr, util::Error> {
        Ok(SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0))
    }
    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }
    async fn close(&self) -> Result<(), util::Error> {
        Ok(())
    }
}

pub(crate) struct VNet {
    pub(crate) wan: Arc<Mutex<router::Router>>,
    pub(crate) net0: Arc<net::Net>,
    pub(crate) net1: Arc<net::Net>,
    pub(crate) server: turn::server::Server,
}

impl VNet {
    pub(crate) async fn close(&self) -> Result<(), Error> {
        self.server.close().await?;
        let mut w = self.wan.lock().await;
        w.stop().await?;
        Ok(())
    }
}

pub(crate) const VNET_GLOBAL_IPA: &str = "27.1.1.1";
pub(crate) const VNET_LOCAL_IPA: &str = "192.168.0.1";
pub(crate) const VNET_LOCAL_SUBNET_MASK_A: &str = "24";
pub(crate) const VNET_GLOBAL_IPB: &str = "28.1.1.1";
pub(crate) const VNET_LOCAL_IPB: &str = "10.2.0.1";
pub(crate) const VNET_LOCAL_SUBNET_MASK_B: &str = "24";
pub(crate) const VNET_STUN_SERVER_IP: &str = "1.2.3.4";
pub(crate) const VNET_STUN_SERVER_PORT: u16 = 3478;

pub(crate) async fn build_simple_vnet(
    _nat_type0: nat::NatType,
    _nat_type1: nat::NatType,
) -> Result<VNet, Error> {
    // WAN
    let wan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "0.0.0.0/0".to_owned(),
        ..Default::default()
    })?));

    let wnet = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ip: VNET_STUN_SERVER_IP.to_owned(), // will be assigned to eth0
        ..Default::default()
    })));

    connect_net2router(&wnet, &wan).await?;

    // LAN
    let lan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: format!("{VNET_LOCAL_IPA}/{VNET_LOCAL_SUBNET_MASK_A}"),
        ..Default::default()
    })?));

    let net0 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec!["192.168.0.1".to_owned()],
        ..Default::default()
    })));
    let net1 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec!["192.168.0.2".to_owned()],
        ..Default::default()
    })));

    connect_net2router(&net0, &lan).await?;
    connect_net2router(&net1, &lan).await?;
    connect_router2router(&lan, &wan).await?;

    // start routers...
    start_router(&wan).await?;

    let server = add_vnet_stun(wnet).await?;

    Ok(VNet {
        wan,
        net0,
        net1,
        server,
    })
}

pub(crate) async fn build_vnet(
    nat_type0: nat::NatType,
    nat_type1: nat::NatType,
) -> Result<VNet, Error> {
    // WAN
    let wan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "0.0.0.0/0".to_owned(),
        ..Default::default()
    })?));

    let wnet = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ip: VNET_STUN_SERVER_IP.to_owned(), // will be assigned to eth0
        ..Default::default()
    })));

    connect_net2router(&wnet, &wan).await?;

    // LAN 0
    let lan0 = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        static_ips: if nat_type0.mode == nat::NatMode::Nat1To1 {
            vec![format!("{VNET_GLOBAL_IPA}/{VNET_LOCAL_IPA}")]
        } else {
            vec![VNET_GLOBAL_IPA.to_owned()]
        },
        cidr: format!("{VNET_LOCAL_IPA}/{VNET_LOCAL_SUBNET_MASK_A}"),
        nat_type: Some(nat_type0),
        ..Default::default()
    })?));

    let net0 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec![VNET_LOCAL_IPA.to_owned()],
        ..Default::default()
    })));

    connect_net2router(&net0, &lan0).await?;
    connect_router2router(&lan0, &wan).await?;

    // LAN 1
    let lan1 = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        static_ips: if nat_type1.mode == nat::NatMode::Nat1To1 {
            vec![format!("{VNET_GLOBAL_IPB}/{VNET_LOCAL_IPB}")]
        } else {
            vec![VNET_GLOBAL_IPB.to_owned()]
        },
        cidr: format!("{VNET_LOCAL_IPB}/{VNET_LOCAL_SUBNET_MASK_B}"),
        nat_type: Some(nat_type1),
        ..Default::default()
    })?));

    let net1 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec![VNET_LOCAL_IPB.to_owned()],
        ..Default::default()
    })));

    connect_net2router(&net1, &lan1).await?;
    connect_router2router(&lan1, &wan).await?;

    // start routers...
    start_router(&wan).await?;

    let server = add_vnet_stun(wnet).await?;

    Ok(VNet {
        wan,
        net0,
        net1,
        server,
    })
}

pub(crate) struct TestAuthHandler {
    pub(crate) cred_map: HashMap<String, Vec<u8>>,
}

impl TestAuthHandler {
    pub(crate) fn new() -> Self {
        let mut cred_map = HashMap::new();
        cred_map.insert(
            "user".to_owned(),
            turn::auth::generate_auth_key("user", "webrtc.rs", "pass"),
        );

        TestAuthHandler { cred_map }
    }
}

impl turn::auth::AuthHandler for TestAuthHandler {
    fn auth_handle(
        &self,
        username: &str,
        _realm: &str,
        _src_addr: SocketAddr,
    ) -> Result<Vec<u8>, turn::Error> {
        if let Some(pw) = self.cred_map.get(username) {
            Ok(pw.to_vec())
        } else {
            Err(turn::Error::Other("fake error".to_owned()))
        }
    }
}

pub(crate) async fn add_vnet_stun(wan_net: Arc<net::Net>) -> Result<turn::server::Server, Error> {
    // Run TURN(STUN) server
    let conn = wan_net
        .bind(SocketAddr::from_str(&format!(
            "{VNET_STUN_SERVER_IP}:{VNET_STUN_SERVER_PORT}"
        ))?)
        .await?;

    let server = turn::server::Server::new(turn::server::config::ServerConfig {
        conn_configs: vec![turn::server::config::ConnConfig {
            conn,
            relay_addr_generator: Box::new(
                turn::relay::relay_static::RelayAddressGeneratorStatic {
                    relay_address: IpAddr::from_str(VNET_STUN_SERVER_IP)?,
                    address: "0.0.0.0".to_owned(),
                    net: wan_net,
                },
            ),
        }],
        realm: "webrtc.rs".to_owned(),
        auth_handler: Arc::new(TestAuthHandler::new()),
        channel_bind_timeout: Duration::from_secs(0),
    })
    .await?;

    Ok(server)
}

pub(crate) async fn connect_with_vnet(
    a_agent: &Arc<Agent>,
    b_agent: &Arc<Agent>,
) -> Result<(Arc<impl Conn>, Arc<impl Conn>), Error> {
    // Manual signaling
    let (a_ufrag, a_pwd) = a_agent.get_local_user_credentials().await;
    let (b_ufrag, b_pwd) = b_agent.get_local_user_credentials().await;

    gather_and_exchange_candidates(a_agent, b_agent).await?;

    let (accepted_tx, mut accepted_rx) = mpsc::channel(1);
    let (_a_cancel_tx, a_cancel_rx) = mpsc::channel(1);

    let agent_a = Arc::clone(a_agent);
    tokio::spawn(async move {
        let a_conn = agent_a.accept(a_cancel_rx, b_ufrag, b_pwd).await?;

        let _ = accepted_tx.send(a_conn).await;

        Result::<(), Error>::Ok(())
    });

    let (_b_cancel_tx, b_cancel_rx) = mpsc::channel(1);
    let b_conn = b_agent.dial(b_cancel_rx, a_ufrag, a_pwd).await?;

    // Ensure accepted
    if let Some(a_conn) = accepted_rx.recv().await {
        Ok((a_conn, b_conn))
    } else {
        Err(Error::Other("no a_conn".to_owned()))
    }
}

#[derive(Default)]
pub(crate) struct AgentTestConfig {
    pub(crate) urls: Vec<Url>,
    pub(crate) nat_1to1_ip_candidate_type: CandidateType,
}

pub(crate) async fn pipe_with_vnet(
    v: &VNet,
    a0test_config: AgentTestConfig,
    a1test_config: AgentTestConfig,
) -> Result<(Arc<impl Conn>, Arc<impl Conn>), Error> {
    let (a_notifier, mut a_connected) = on_connected();
    let (b_notifier, mut b_connected) = on_connected();

    let nat_1to1_ips = if a0test_config.nat_1to1_ip_candidate_type != CandidateType::Unspecified {
        vec![VNET_GLOBAL_IPA.to_owned()]
    } else {
        vec![]
    };

    let cfg0 = AgentConfig {
        urls: a0test_config.urls,
        network_types: supported_network_types(),
        multicast_dns_mode: MulticastDnsMode::Disabled,
        nat_1to1_ips,
        nat_1to1_ip_candidate_type: a0test_config.nat_1to1_ip_candidate_type,
        net: Some(Arc::clone(&v.net0)),
        ..Default::default()
    };

    let a_agent = Arc::new(Agent::new(cfg0).await?);
    a_agent.on_connection_state_change(a_notifier);

    let nat_1to1_ips = if a1test_config.nat_1to1_ip_candidate_type != CandidateType::Unspecified {
        vec![VNET_GLOBAL_IPB.to_owned()]
    } else {
        vec![]
    };
    let cfg1 = AgentConfig {
        urls: a1test_config.urls,
        network_types: supported_network_types(),
        multicast_dns_mode: MulticastDnsMode::Disabled,
        nat_1to1_ips,
        nat_1to1_ip_candidate_type: a1test_config.nat_1to1_ip_candidate_type,
        net: Some(Arc::clone(&v.net1)),
        ..Default::default()
    };

    let b_agent = Arc::new(Agent::new(cfg1).await?);
    b_agent.on_connection_state_change(b_notifier);

    let (a_conn, b_conn) = connect_with_vnet(&a_agent, &b_agent).await?;

    // Ensure pair selected
    // Note: this assumes ConnectionStateConnected is thrown after selecting the final pair
    let _ = a_connected.recv().await;
    let _ = b_connected.recv().await;

    Ok((a_conn, b_conn))
}

pub(crate) fn on_connected() -> (OnConnectionStateChangeHdlrFn, mpsc::Receiver<()>) {
    let (done_tx, done_rx) = mpsc::channel::<()>(1);
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    let hdlr_fn: OnConnectionStateChangeHdlrFn = Box::new(move |state: ConnectionState| {
        let done_tx_clone = Arc::clone(&done_tx);
        Box::pin(async move {
            if state == ConnectionState::Connected {
                let mut tx = done_tx_clone.lock().await;
                tx.take();
            }
        })
    });
    (hdlr_fn, done_rx)
}

pub(crate) async fn gather_and_exchange_candidates(
    a_agent: &Arc<Agent>,
    b_agent: &Arc<Agent>,
) -> Result<(), Error> {
    let wg = WaitGroup::new();

    let w1 = Arc::new(Mutex::new(Some(wg.worker())));
    a_agent.on_candidate(Box::new(
        move |candidate: Option<Arc<dyn Candidate + Send + Sync>>| {
            let w3 = Arc::clone(&w1);
            Box::pin(async move {
                if candidate.is_none() {
                    let mut w = w3.lock().await;
                    w.take();
                }
            })
        },
    ));
    a_agent.gather_candidates()?;

    let w2 = Arc::new(Mutex::new(Some(wg.worker())));
    b_agent.on_candidate(Box::new(
        move |candidate: Option<Arc<dyn Candidate + Send + Sync>>| {
            let w3 = Arc::clone(&w2);
            Box::pin(async move {
                if candidate.is_none() {
                    let mut w = w3.lock().await;
                    w.take();
                }
            })
        },
    ));
    b_agent.gather_candidates()?;

    wg.wait().await;

    let candidates = a_agent.get_local_candidates().await?;
    for c in candidates {
        let c2: Arc<dyn Candidate + Send + Sync> =
            Arc::new(unmarshal_candidate(c.marshal().as_str())?);
        b_agent.add_remote_candidate(&c2)?;
    }

    let candidates = b_agent.get_local_candidates().await?;
    for c in candidates {
        let c2: Arc<dyn Candidate + Send + Sync> =
            Arc::new(unmarshal_candidate(c.marshal().as_str())?);
        a_agent.add_remote_candidate(&c2)?;
    }

    Ok(())
}

pub(crate) async fn start_router(router: &Arc<Mutex<router::Router>>) -> Result<(), Error> {
    let mut w = router.lock().await;
    Ok(w.start().await?)
}

pub(crate) async fn connect_net2router(
    net: &Arc<net::Net>,
    router: &Arc<Mutex<router::Router>>,
) -> Result<(), Error> {
    let nic = net.get_nic()?;

    {
        let mut w = router.lock().await;
        w.add_net(Arc::clone(&nic)).await?;
    }
    {
        let n = nic.lock().await;
        n.set_router(Arc::clone(router)).await?;
    }

    Ok(())
}

pub(crate) async fn connect_router2router(
    child: &Arc<Mutex<router::Router>>,
    parent: &Arc<Mutex<router::Router>>,
) -> Result<(), Error> {
    {
        let mut w = parent.lock().await;
        w.add_router(Arc::clone(child)).await?;
    }

    {
        let l = child.lock().await;
        l.set_router(Arc::clone(parent)).await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_connectivity_simple_vnet_full_cone_nats_on_both_ends() -> Result<(), Error> {
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

    let stun_server_url = Url {
        scheme: SchemeType::Stun,
        host: VNET_STUN_SERVER_IP.to_owned(),
        port: VNET_STUN_SERVER_PORT,
        proto: ProtoType::Udp,
        ..Default::default()
    };

    // buildVNet with a Full-cone NATs both LANs
    let nat_type = nat::NatType {
        mapping_behavior: nat::EndpointDependencyType::EndpointIndependent,
        filtering_behavior: nat::EndpointDependencyType::EndpointIndependent,
        ..Default::default()
    };

    let v = build_simple_vnet(nat_type, nat_type).await?;

    log::debug!("Connecting...");
    let a0test_config = AgentTestConfig {
        urls: vec![stun_server_url.clone()],
        ..Default::default()
    };
    let a1test_config = AgentTestConfig {
        urls: vec![stun_server_url.clone()],
        ..Default::default()
    };
    let (_ca, _cb) = pipe_with_vnet(&v, a0test_config, a1test_config).await?;

    tokio::time::sleep(Duration::from_secs(1)).await;

    log::debug!("Closing...");
    v.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_connectivity_vnet_full_cone_nats_on_both_ends() -> Result<(), Error> {
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

    let stun_server_url = Url {
        scheme: SchemeType::Stun,
        host: VNET_STUN_SERVER_IP.to_owned(),
        port: VNET_STUN_SERVER_PORT,
        proto: ProtoType::Udp,
        ..Default::default()
    };

    let _turn_server_url = Url {
        scheme: SchemeType::Turn,
        host: VNET_STUN_SERVER_IP.to_owned(),
        port: VNET_STUN_SERVER_PORT,
        username: "user".to_owned(),
        password: "pass".to_owned(),
        proto: ProtoType::Udp,
    };

    // buildVNet with a Full-cone NATs both LANs
    let nat_type = nat::NatType {
        mapping_behavior: nat::EndpointDependencyType::EndpointIndependent,
        filtering_behavior: nat::EndpointDependencyType::EndpointIndependent,
        ..Default::default()
    };

    let v = build_vnet(nat_type, nat_type).await?;

    log::debug!("Connecting...");
    let a0test_config = AgentTestConfig {
        urls: vec![stun_server_url.clone()],
        ..Default::default()
    };
    let a1test_config = AgentTestConfig {
        urls: vec![stun_server_url.clone()],
        ..Default::default()
    };
    let (_ca, _cb) = pipe_with_vnet(&v, a0test_config, a1test_config).await?;

    tokio::time::sleep(Duration::from_secs(1)).await;

    log::debug!("Closing...");
    v.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_connectivity_vnet_symmetric_nats_on_both_ends() -> Result<(), Error> {
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

    let stun_server_url = Url {
        scheme: SchemeType::Stun,
        host: VNET_STUN_SERVER_IP.to_owned(),
        port: VNET_STUN_SERVER_PORT,
        proto: ProtoType::Udp,
        ..Default::default()
    };

    let turn_server_url = Url {
        scheme: SchemeType::Turn,
        host: VNET_STUN_SERVER_IP.to_owned(),
        port: VNET_STUN_SERVER_PORT,
        username: "user".to_owned(),
        password: "pass".to_owned(),
        proto: ProtoType::Udp,
    };

    // buildVNet with a Symmetric NATs for both LANs
    let nat_type = nat::NatType {
        mapping_behavior: nat::EndpointDependencyType::EndpointAddrPortDependent,
        filtering_behavior: nat::EndpointDependencyType::EndpointAddrPortDependent,
        ..Default::default()
    };

    let v = build_vnet(nat_type, nat_type).await?;

    log::debug!("Connecting...");
    let a0test_config = AgentTestConfig {
        urls: vec![stun_server_url.clone(), turn_server_url.clone()],
        ..Default::default()
    };
    let a1test_config = AgentTestConfig {
        urls: vec![stun_server_url.clone()],
        ..Default::default()
    };
    let (_ca, _cb) = pipe_with_vnet(&v, a0test_config, a1test_config).await?;

    tokio::time::sleep(Duration::from_secs(1)).await;

    log::debug!("Closing...");
    v.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_connectivity_vnet_1to1_nat_with_host_candidate_vs_symmetric_nats() -> Result<(), Error>
{
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

    // Agent0 is behind 1:1 NAT
    let nat_type0 = nat::NatType {
        mode: nat::NatMode::Nat1To1,
        ..Default::default()
    };
    // Agent1 is behind a symmetric NAT
    let nat_type1 = nat::NatType {
        mapping_behavior: nat::EndpointDependencyType::EndpointAddrPortDependent,
        filtering_behavior: nat::EndpointDependencyType::EndpointAddrPortDependent,
        ..Default::default()
    };
    log::debug!("natType0: {:?}", nat_type0);
    log::debug!("natType1: {:?}", nat_type1);

    let v = build_vnet(nat_type0, nat_type1).await?;

    log::debug!("Connecting...");
    let a0test_config = AgentTestConfig {
        urls: vec![],
        nat_1to1_ip_candidate_type: CandidateType::Host, // Use 1:1 NAT IP as a host candidate
    };
    let a1test_config = AgentTestConfig {
        urls: vec![],
        ..Default::default()
    };
    let (_ca, _cb) = pipe_with_vnet(&v, a0test_config, a1test_config).await?;

    tokio::time::sleep(Duration::from_secs(1)).await;

    log::debug!("Closing...");
    v.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_connectivity_vnet_1to1_nat_with_srflx_candidate_vs_symmetric_nats(
) -> Result<(), Error> {
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

    // Agent0 is behind 1:1 NAT
    let nat_type0 = nat::NatType {
        mode: nat::NatMode::Nat1To1,
        ..Default::default()
    };
    // Agent1 is behind a symmetric NAT
    let nat_type1 = nat::NatType {
        mapping_behavior: nat::EndpointDependencyType::EndpointAddrPortDependent,
        filtering_behavior: nat::EndpointDependencyType::EndpointAddrPortDependent,
        ..Default::default()
    };
    log::debug!("natType0: {:?}", nat_type0);
    log::debug!("natType1: {:?}", nat_type1);

    let v = build_vnet(nat_type0, nat_type1).await?;

    log::debug!("Connecting...");
    let a0test_config = AgentTestConfig {
        urls: vec![],
        nat_1to1_ip_candidate_type: CandidateType::ServerReflexive, // Use 1:1 NAT IP as a srflx candidate
    };
    let a1test_config = AgentTestConfig {
        urls: vec![],
        ..Default::default()
    };
    let (_ca, _cb) = pipe_with_vnet(&v, a0test_config, a1test_config).await?;

    tokio::time::sleep(Duration::from_secs(1)).await;

    log::debug!("Closing...");
    v.close().await?;

    Ok(())
}

async fn block_until_state_seen(
    expected_state: ConnectionState,
    state_queue: &mut mpsc::Receiver<ConnectionState>,
) {
    while let Some(s) = state_queue.recv().await {
        if s == expected_state {
            return;
        }
    }
}

// test_disconnected_to_connected asserts that an agent can go to disconnected, and then return to connected successfully
#[tokio::test]
async fn test_disconnected_to_connected() -> Result<(), Error> {
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

    // Create a network with two interfaces
    let wan = router::Router::new(router::RouterConfig {
        cidr: "0.0.0.0/0".to_owned(),
        ..Default::default()
    })?;

    let drop_all_data = Arc::new(AtomicU64::new(0));
    let drop_all_data2 = Arc::clone(&drop_all_data);
    wan.add_chunk_filter(Box::new(move |_c: &(dyn Chunk + Send + Sync)| -> bool {
        drop_all_data2.load(Ordering::SeqCst) != 1
    }))
    .await;
    let wan = Arc::new(Mutex::new(wan));

    let net0 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec!["192.168.0.1".to_owned()],
        ..Default::default()
    })));
    let net1 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec!["192.168.0.2".to_owned()],
        ..Default::default()
    })));

    connect_net2router(&net0, &wan).await?;
    connect_net2router(&net1, &wan).await?;
    start_router(&wan).await?;

    let disconnected_timeout = Duration::from_secs(1);
    let keepalive_interval = Duration::from_millis(20);

    // Create two agents and connect them
    let controlling_agent = Arc::new(
        Agent::new(AgentConfig {
            network_types: supported_network_types(),
            multicast_dns_mode: MulticastDnsMode::Disabled,
            net: Some(Arc::clone(&net0)),
            disconnected_timeout: Some(disconnected_timeout),
            keepalive_interval: Some(keepalive_interval),
            check_interval: keepalive_interval,
            ..Default::default()
        })
        .await?,
    );

    let controlled_agent = Arc::new(
        Agent::new(AgentConfig {
            network_types: supported_network_types(),
            multicast_dns_mode: MulticastDnsMode::Disabled,
            net: Some(Arc::clone(&net1)),
            disconnected_timeout: Some(disconnected_timeout),
            keepalive_interval: Some(keepalive_interval),
            check_interval: keepalive_interval,
            ..Default::default()
        })
        .await?,
    );

    let (controlling_state_changes_tx, mut controlling_state_changes_rx) =
        mpsc::channel::<ConnectionState>(100);
    let controlling_state_changes_tx = Arc::new(controlling_state_changes_tx);
    controlling_agent.on_connection_state_change(Box::new(move |c: ConnectionState| {
        let controlling_state_changes_tx_clone = Arc::clone(&controlling_state_changes_tx);
        Box::pin(async move {
            let _ = controlling_state_changes_tx_clone.try_send(c);
        })
    }));

    let (controlled_state_changes_tx, mut controlled_state_changes_rx) =
        mpsc::channel::<ConnectionState>(100);
    let controlled_state_changes_tx = Arc::new(controlled_state_changes_tx);
    controlled_agent.on_connection_state_change(Box::new(move |c: ConnectionState| {
        let controlled_state_changes_tx_clone = Arc::clone(&controlled_state_changes_tx);
        Box::pin(async move {
            let _ = controlled_state_changes_tx_clone.try_send(c);
        })
    }));

    connect_with_vnet(&controlling_agent, &controlled_agent).await?;

    // Assert we have gone to connected
    block_until_state_seen(
        ConnectionState::Connected,
        &mut controlling_state_changes_rx,
    )
    .await;
    block_until_state_seen(ConnectionState::Connected, &mut controlled_state_changes_rx).await;

    // Drop all packets, and block until we have gone to disconnected
    drop_all_data.store(1, Ordering::SeqCst);
    block_until_state_seen(
        ConnectionState::Disconnected,
        &mut controlling_state_changes_rx,
    )
    .await;
    block_until_state_seen(
        ConnectionState::Disconnected,
        &mut controlled_state_changes_rx,
    )
    .await;

    // Allow all packets through again, block until we have gone to connected
    drop_all_data.store(0, Ordering::SeqCst);
    block_until_state_seen(
        ConnectionState::Connected,
        &mut controlling_state_changes_rx,
    )
    .await;
    block_until_state_seen(ConnectionState::Connected, &mut controlled_state_changes_rx).await;

    {
        let mut w = wan.lock().await;
        w.stop().await?;
    }

    controlling_agent.close().await?;
    controlled_agent.close().await?;

    Ok(())
}

//use std::io::Write;

// Agent.Write should use the best valid pair if a selected pair is not yet available
#[tokio::test]
async fn test_write_use_valid_pair() -> Result<(), Error> {
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

    // Create a network with two interfaces
    let wan = router::Router::new(router::RouterConfig {
        cidr: "0.0.0.0/0".to_owned(),
        ..Default::default()
    })?;

    wan.add_chunk_filter(Box::new(move |c: &(dyn Chunk + Send + Sync)| -> bool {
        let raw = c.user_data();
        if stun::message::is_message(&raw) {
            let mut m = stun::message::Message {
                raw,
                ..Default::default()
            };
            let result = m.decode();
            if result.is_err() | m.contains(stun::attributes::ATTR_USE_CANDIDATE) {
                return false;
            }
        }

        true
    }))
    .await;
    let wan = Arc::new(Mutex::new(wan));

    let net0 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec!["192.168.0.1".to_owned()],
        ..Default::default()
    })));
    let net1 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec!["192.168.0.2".to_owned()],
        ..Default::default()
    })));

    connect_net2router(&net0, &wan).await?;
    connect_net2router(&net1, &wan).await?;
    start_router(&wan).await?;

    // Create two agents and connect them
    let controlling_agent = Arc::new(
        Agent::new(AgentConfig {
            network_types: supported_network_types(),
            multicast_dns_mode: MulticastDnsMode::Disabled,
            net: Some(Arc::clone(&net0)),
            ..Default::default()
        })
        .await?,
    );

    let controlled_agent = Arc::new(
        Agent::new(AgentConfig {
            network_types: supported_network_types(),
            multicast_dns_mode: MulticastDnsMode::Disabled,
            net: Some(Arc::clone(&net1)),
            ..Default::default()
        })
        .await?,
    );

    gather_and_exchange_candidates(&controlling_agent, &controlled_agent).await?;

    let (controlling_ufrag, controlling_pwd) = controlling_agent.get_local_user_credentials().await;
    let (controlled_ufrag, controlled_pwd) = controlled_agent.get_local_user_credentials().await;

    let controlling_agent_tx = Arc::clone(&controlling_agent);
    tokio::spawn(async move {
        let test_message = "Test Message";
        let controlling_agent_conn = {
            controlling_agent_tx
                .internal
                .start_connectivity_checks(true, controlled_ufrag, controlled_pwd)
                .await?;
            Arc::clone(&controlling_agent_tx.internal.agent_conn) as Arc<dyn Conn + Send + Sync>
        };

        log::debug!("controlling_agent start_connectivity_checks done...");
        loop {
            let result = controlling_agent_conn.send(test_message.as_bytes()).await;
            if result.is_err() {
                break;
            }

            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        Result::<(), Error>::Ok(())
    });

    let controlled_agent_conn = {
        controlled_agent
            .internal
            .start_connectivity_checks(false, controlling_ufrag, controlling_pwd)
            .await?;
        Arc::clone(&controlled_agent.internal.agent_conn) as Arc<dyn Conn + Send + Sync>
    };

    log::debug!("controlled_agent start_connectivity_checks done...");

    let test_message = "Test Message";
    let mut read_buf = vec![0u8; test_message.as_bytes().len()];
    controlled_agent_conn.recv(&mut read_buf).await?;

    assert_eq!(read_buf, test_message.as_bytes(), "should match");

    {
        let mut w = wan.lock().await;
        w.stop().await?;
    }

    controlling_agent.close().await?;
    controlled_agent.close().await?;

    Ok(())
}
