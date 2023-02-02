use super::config::*;
use super::*;
use crate::auth::generate_auth_key;
use crate::client::*;
use crate::error::*;
use crate::relay::relay_static::*;

use crate::relay::relay_none::RelayAddressGeneratorNone;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use util::{vnet::router::Nic, vnet::*};

struct TestAuthHandler {
    cred_map: HashMap<String, Vec<u8>>,
}

impl TestAuthHandler {
    fn new() -> Self {
        let mut cred_map = HashMap::new();
        cred_map.insert(
            "user".to_owned(),
            generate_auth_key("user", "webrtc.rs", "pass"),
        );

        TestAuthHandler { cred_map }
    }
}

impl AuthHandler for TestAuthHandler {
    fn auth_handle(&self, username: &str, _realm: &str, _src_addr: SocketAddr) -> Result<Vec<u8>> {
        if let Some(pw) = self.cred_map.get(username) {
            Ok(pw.to_vec())
        } else {
            Err(Error::ErrFakeErr)
        }
    }
}

#[tokio::test]
async fn test_server_simple() -> Result<()> {
    // here, it should use static port, like "0.0.0.0:3478",
    // but, due to different test environment, let's fake it by using "0.0.0.0:0"
    // to auto assign a "static" port
    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let server_port = conn.local_addr()?.port();

    let server = Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorStatic {
                relay_address: IpAddr::from_str("127.0.0.1")?,
                address: "0.0.0.0".to_owned(),
                net: Arc::new(net::Net::new(None)),
            }),
        }],
        realm: "webrtc.rs".to_owned(),
        auth_handler: Arc::new(TestAuthHandler::new()),
        channel_bind_timeout: Duration::from_secs(0),
    })
    .await?;

    assert_eq!(
        DEFAULT_LIFETIME, server.channel_bind_timeout,
        "should match"
    );

    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let client = Client::new(ClientConfig {
        stun_serv_addr: String::new(),
        turn_serv_addr: String::new(),
        username: String::new(),
        password: String::new(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 0,
        conn,
        vnet: None,
    })
    .await?;

    client.listen().await?;

    client
        .send_binding_request_to(format!("127.0.0.1:{server_port}").as_str())
        .await?;

    client.close().await?;
    server.close().await?;

    Ok(())
}

struct VNet {
    wan: Arc<Mutex<router::Router>>,
    net0: Arc<net::Net>,
    net1: Arc<net::Net>,
    netl0: Arc<net::Net>,
    server: Server,
}

async fn build_vnet() -> Result<VNet> {
    // WAN
    let wan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "0.0.0.0/0".to_owned(),
        ..Default::default()
    })?));

    let net0 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ip: "1.2.3.4".to_owned(), // will be assigned to eth0
        ..Default::default()
    })));

    let net1 = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ip: "1.2.3.5".to_owned(), // will be assigned to eth0
        ..Default::default()
    })));

    {
        let nic0 = net0.get_nic()?;
        let nic1 = net1.get_nic()?;

        {
            let mut w = wan.lock().await;
            w.add_net(Arc::clone(&nic0)).await?;
            w.add_net(Arc::clone(&nic1)).await?;
        }

        let n0 = nic0.lock().await;
        n0.set_router(Arc::clone(&wan)).await?;

        let n1 = nic1.lock().await;
        n1.set_router(Arc::clone(&wan)).await?;
    }

    // LAN
    let lan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        static_ip: "5.6.7.8".to_owned(), // this router's external IP on eth0
        cidr: "192.168.0.0/24".to_owned(),
        nat_type: Some(nat::NatType {
            mapping_behavior: nat::EndpointDependencyType::EndpointIndependent,
            filtering_behavior: nat::EndpointDependencyType::EndpointIndependent,
            ..Default::default()
        }),
        ..Default::default()
    })?));

    let netl0 = Arc::new(net::Net::new(Some(net::NetConfig::default())));

    {
        let nic = netl0.get_nic()?;

        {
            let mut l = lan.lock().await;
            l.add_net(Arc::clone(&nic)).await?;
        }

        let n = nic.lock().await;
        n.set_router(Arc::clone(&lan)).await?;
    }

    {
        {
            let mut w = wan.lock().await;
            w.add_router(Arc::clone(&lan)).await?;
        }

        {
            let l = lan.lock().await;
            l.set_router(Arc::clone(&wan)).await?;
        }
    }

    {
        let mut w = wan.lock().await;
        w.start().await?;
    }

    // start server...
    let conn = net0.bind(SocketAddr::from_str("0.0.0.0:3478")?).await?;

    let server = Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorNone {
                address: "1.2.3.4".to_owned(),
                net: Arc::clone(&net0),
            }),
        }],
        realm: "webrtc.rs".to_owned(),
        auth_handler: Arc::new(TestAuthHandler::new()),
        channel_bind_timeout: Duration::from_secs(0),
    })
    .await?;

    // register host names
    {
        let mut w = wan.lock().await;
        w.add_host("stun.webrtc.rs".to_owned(), "1.2.3.4".to_owned())
            .await?;
        w.add_host("turn.webrtc.rs".to_owned(), "1.2.3.4".to_owned())
            .await?;
        w.add_host("echo.webrtc.rs".to_owned(), "1.2.3.5".to_owned())
            .await?;
    }

    Ok(VNet {
        wan,
        net0,
        net1,
        netl0,
        server,
    })
}

#[tokio::test]
async fn test_server_vnet_send_binding_request() -> Result<()> {
    let v = build_vnet().await?;

    let lconn = v.netl0.bind(SocketAddr::from_str("0.0.0.0:0")?).await?;
    log::debug!("creating a client.");
    let client = Client::new(ClientConfig {
        stun_serv_addr: "1.2.3.4:3478".to_owned(),
        turn_serv_addr: String::new(),
        username: String::new(),
        password: String::new(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 0,
        conn: lconn,
        vnet: Some(Arc::clone(&v.netl0)),
    })
    .await?;

    client.listen().await?;

    log::debug!("sending a binding request.");
    let refl_addr = client.send_binding_request().await?;
    log::debug!("mapped-address: {}", refl_addr);

    // The mapped-address should have IP address that was assigned
    // to the LAN router.
    assert_eq!(
        refl_addr.ip().to_string(),
        Ipv4Addr::new(5, 6, 7, 8).to_string(),
        "should match",
    );

    client.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_server_vnet_echo_via_relay() -> Result<()> {
    let v = build_vnet().await?;

    let lconn = v.netl0.bind(SocketAddr::from_str("0.0.0.0:0")?).await?;
    log::debug!("creating a client.");
    let client = Client::new(ClientConfig {
        stun_serv_addr: "stun.webrtc.rs:3478".to_owned(),
        turn_serv_addr: "turn.webrtc.rs:3478".to_owned(),
        username: "user".to_owned(),
        password: "pass".to_owned(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 0,
        conn: lconn,
        vnet: Some(Arc::clone(&v.netl0)),
    })
    .await?;

    client.listen().await?;

    log::debug!("sending a binding request.");
    let conn = client.allocate().await?;
    let local_addr = conn.local_addr()?;

    log::debug!("laddr: {}", conn.local_addr()?);

    let echo_conn = v.net1.bind(SocketAddr::from_str("1.2.3.5:5678")?).await?;
    let echo_addr = echo_conn.local_addr()?;

    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);

    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        let mut n;
        let mut from;
        loop {
            tokio::select! {
                _ = done_rx.recv() => break,
                result = echo_conn.recv_from(&mut buf) => {
                    match result {
                        Ok((s, addr)) => {
                            n = s;
                            from = addr;
                        }
                        Err(_) => break,
                    }
                }
            }

            // verify the message was received from the relay address
            assert_eq!(local_addr.to_string(), from.to_string(), "should match");
            assert_eq!(b"Hello", &buf[..n], "should match");

            // echo the data
            let _ = echo_conn.send_to(&buf[..n], from).await;
        }
    });

    let mut buf = vec![0u8; 1500];

    for _ in 0..10 {
        log::debug!("sending \"Hello\"..");
        conn.send_to(b"Hello", echo_addr).await?;

        let (_, from) = conn.recv_from(&mut buf).await?;

        // verify the message was received from the relay address
        assert_eq!(echo_addr.to_string(), from.to_string(), "should match");

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    client.close().await?;
    drop(done_tx);

    Ok(())
}
