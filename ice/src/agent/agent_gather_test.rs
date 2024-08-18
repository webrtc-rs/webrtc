use std::str::FromStr;

use ipnet::IpNet;
use tokio::net::UdpSocket;
use util::vnet::*;

use super::agent_vnet_test::*;
use super::*;
use crate::udp_mux::{UDPMuxDefault, UDPMuxParams};
use crate::util::*;

#[tokio::test]
async fn test_vnet_gather_no_local_ip_address() -> Result<()> {
    let vnet = Arc::new(net::Net::new(Some(net::NetConfig::default())));

    let a = Agent::new(AgentConfig {
        net: Some(Arc::clone(&vnet)),
        ..Default::default()
    })
    .await?;

    let local_ips = local_interfaces(
        &vnet,
        &a.interface_filter,
        &a.ip_filter,
        &[NetworkType::Udp4],
        false,
    )
    .await;
    assert!(local_ips.is_empty(), "should return no local IP");

    a.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_vnet_gather_dynamic_ip_address() -> Result<()> {
    let cider = "1.2.3.0/24";
    let ipnet = IpNet::from_str(cider).map_err(|e| Error::Other(e.to_string()))?;

    let r = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: cider.to_owned(),
        ..Default::default()
    })?));
    let nw = Arc::new(net::Net::new(Some(net::NetConfig::default())));
    connect_net2router(&nw, &r).await?;

    let a = Agent::new(AgentConfig {
        net: Some(Arc::clone(&nw)),
        ..Default::default()
    })
    .await?;

    let local_ips = local_interfaces(
        &nw,
        &a.interface_filter,
        &a.ip_filter,
        &[NetworkType::Udp4],
        false,
    )
    .await;
    assert!(!local_ips.is_empty(), "should have one local IP");

    for ip in &local_ips {
        if ip.is_loopback() {
            panic!("should not return loopback IP");
        }
        if !ipnet.contains(ip) {
            panic!("{ip} should be contained in the CIDR {ipnet}");
        }
    }

    a.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_vnet_gather_listen_udp() -> Result<()> {
    let cider = "1.2.3.0/24";
    let r = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: cider.to_owned(),
        ..Default::default()
    })?));
    let nw = Arc::new(net::Net::new(Some(net::NetConfig::default())));
    connect_net2router(&nw, &r).await?;

    let a = Agent::new(AgentConfig {
        net: Some(Arc::clone(&nw)),
        ..Default::default()
    })
    .await?;

    let local_ips = local_interfaces(
        &nw,
        &a.interface_filter,
        &a.ip_filter,
        &[NetworkType::Udp4],
        false,
    )
    .await;
    assert!(!local_ips.is_empty(), "should have one local IP");

    for ip in local_ips {
        let _ = listen_udp_in_port_range(&nw, 0, 0, SocketAddr::new(ip, 0)).await?;

        let result = listen_udp_in_port_range(&nw, 4999, 5000, SocketAddr::new(ip, 0)).await;
        assert!(
            result.is_err(),
            "listenUDP with invalid port range did not return ErrPort"
        );

        let conn = listen_udp_in_port_range(&nw, 5000, 5000, SocketAddr::new(ip, 0)).await?;
        let port = conn.local_addr()?.port();
        assert_eq!(
            port, 5000,
            "listenUDP with port restriction of 5000 listened on incorrect port ({port})"
        );
    }

    a.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_vnet_gather_with_nat_1to1_as_host_candidates() -> Result<()> {
    let external_ip0 = "1.2.3.4";
    let external_ip1 = "1.2.3.5";
    let local_ip0 = "10.0.0.1";
    let local_ip1 = "10.0.0.2";
    let map0 = format!("{external_ip0}/{local_ip0}");
    let map1 = format!("{external_ip1}/{local_ip1}");

    let wan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "1.2.3.0/24".to_owned(),
        ..Default::default()
    })?));

    let lan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "10.0.0.0/24".to_owned(),
        static_ips: vec![map0.clone(), map1.clone()],
        nat_type: Some(nat::NatType {
            mode: nat::NatMode::Nat1To1,
            ..Default::default()
        }),
        ..Default::default()
    })?));

    connect_router2router(&lan, &wan).await?;

    let nw = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec![local_ip0.to_owned(), local_ip1.to_owned()],
        ..Default::default()
    })));

    connect_net2router(&nw, &lan).await?;

    let a = Agent::new(AgentConfig {
        network_types: vec![NetworkType::Udp4],
        nat_1to1_ips: vec![map0.clone(), map1.clone()],
        net: Some(Arc::clone(&nw)),
        ..Default::default()
    })
    .await?;

    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    a.on_candidate(Box::new(
        move |c: Option<Arc<dyn Candidate + Send + Sync>>| {
            let done_tx_clone = Arc::clone(&done_tx);
            Box::pin(async move {
                if c.is_none() {
                    let mut tx = done_tx_clone.lock().await;
                    tx.take();
                }
            })
        },
    ));

    a.gather_candidates()?;

    log::debug!("wait for gathering is done...");
    let _ = done_rx.recv().await;
    log::debug!("gathering is done");

    let candidates = a.get_local_candidates().await?;
    assert_eq!(candidates.len(), 2, "There must be two candidates");

    let mut laddrs = vec![];
    for candi in &candidates {
        if let Some(conn) = candi.get_conn() {
            let laddr = conn.local_addr()?;
            assert_eq!(
                candi.port(),
                laddr.port(),
                "Unexpected candidate port: {}",
                candi.port()
            );
            laddrs.push(laddr);
        }
    }

    if candidates[0].address() == external_ip0 {
        assert_eq!(
            candidates[1].address(),
            external_ip1,
            "Unexpected candidate IP: {}",
            candidates[1].address()
        );
        assert_eq!(
            laddrs[0].ip().to_string(),
            local_ip0,
            "Unexpected listen IP: {}",
            laddrs[0].ip()
        );
        assert_eq!(
            laddrs[1].ip().to_string(),
            local_ip1,
            "Unexpected listen IP: {}",
            laddrs[1].ip()
        );
    } else if candidates[0].address() == external_ip1 {
        assert_eq!(
            candidates[1].address(),
            external_ip0,
            "Unexpected candidate IP: {}",
            candidates[1].address()
        );
        assert_eq!(
            laddrs[0].ip().to_string(),
            local_ip1,
            "Unexpected listen IP: {}",
            laddrs[0].ip(),
        );
        assert_eq!(
            laddrs[1].ip().to_string(),
            local_ip0,
            "Unexpected listen IP: {}",
            laddrs[1].ip(),
        )
    }

    a.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_vnet_gather_with_nat_1to1_as_srflx_candidates() -> Result<()> {
    let wan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "1.2.3.0/24".to_owned(),
        ..Default::default()
    })?));

    let lan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "10.0.0.0/24".to_owned(),
        static_ips: vec!["1.2.3.4/10.0.0.1".to_owned()],
        nat_type: Some(nat::NatType {
            mode: nat::NatMode::Nat1To1,
            ..Default::default()
        }),
        ..Default::default()
    })?));

    connect_router2router(&lan, &wan).await?;

    let nw = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec!["10.0.0.1".to_owned()],
        ..Default::default()
    })));

    connect_net2router(&nw, &lan).await?;

    let a = Agent::new(AgentConfig {
        network_types: vec![NetworkType::Udp4],
        nat_1to1_ips: vec!["1.2.3.4".to_owned()],
        nat_1to1_ip_candidate_type: CandidateType::ServerReflexive,
        net: Some(nw),
        ..Default::default()
    })
    .await?;

    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    a.on_candidate(Box::new(
        move |c: Option<Arc<dyn Candidate + Send + Sync>>| {
            let done_tx_clone = Arc::clone(&done_tx);
            Box::pin(async move {
                if c.is_none() {
                    let mut tx = done_tx_clone.lock().await;
                    tx.take();
                }
            })
        },
    ));

    a.gather_candidates()?;

    log::debug!("wait for gathering is done...");
    let _ = done_rx.recv().await;
    log::debug!("gathering is done");

    let candidates = a.get_local_candidates().await?;
    assert_eq!(candidates.len(), 2, "There must be two candidates");

    let mut candi_host = None;
    let mut candi_srflx = None;

    for candidate in candidates {
        match candidate.candidate_type() {
            CandidateType::Host => {
                candi_host = Some(candidate);
            }
            CandidateType::ServerReflexive => {
                candi_srflx = Some(candidate);
            }
            _ => {
                panic!("Unexpected candidate type");
            }
        }
    }

    assert!(candi_host.is_some(), "should not be nil");
    assert_eq!("10.0.0.1", candi_host.unwrap().address(), "should match");
    assert!(candi_srflx.is_some(), "should not be nil");
    assert_eq!("1.2.3.4", candi_srflx.unwrap().address(), "should match");

    a.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_vnet_gather_with_interface_filter() -> Result<()> {
    let r = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "1.2.3.0/24".to_owned(),
        ..Default::default()
    })?));
    let nw = Arc::new(net::Net::new(Some(net::NetConfig::default())));
    connect_net2router(&nw, &r).await?;

    //"InterfaceFilter should exclude the interface"
    {
        let a = Agent::new(AgentConfig {
            net: Some(Arc::clone(&nw)),
            interface_filter: Arc::new(Some(Box::new(|_: &str| -> bool {
                //assert_eq!("eth0", interface_name);
                false
            }))),
            ..Default::default()
        })
        .await?;

        let local_ips = local_interfaces(
            &nw,
            &a.interface_filter,
            &a.ip_filter,
            &[NetworkType::Udp4],
            false,
        )
        .await;
        assert!(
            local_ips.is_empty(),
            "InterfaceFilter should have excluded everything"
        );

        a.close().await?;
    }

    //"InterfaceFilter should not exclude the interface"
    {
        let a = Agent::new(AgentConfig {
            net: Some(Arc::clone(&nw)),
            interface_filter: Arc::new(Some(Box::new(|interface_name: &str| -> bool {
                "eth0" == interface_name
            }))),
            ..Default::default()
        })
        .await?;

        let local_ips = local_interfaces(
            &nw,
            &a.interface_filter,
            &a.ip_filter,
            &[NetworkType::Udp4],
            false,
        )
        .await;
        assert_eq!(
            local_ips.len(),
            1,
            "InterfaceFilter should not have excluded everything"
        );

        a.close().await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_vnet_gather_turn_connection_leak() -> Result<()> {
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

    let cfg0 = AgentConfig {
        urls: vec![turn_server_url.clone()],
        network_types: supported_network_types(),
        multicast_dns_mode: MulticastDnsMode::Disabled,
        nat_1to1_ips: vec![VNET_GLOBAL_IPA.to_owned()],
        net: Some(Arc::clone(&v.net0)),
        ..Default::default()
    };

    let a_agent = Agent::new(cfg0).await?;

    {
        let agent_internal = Arc::clone(&a_agent.internal);
        Agent::gather_candidates_relay(
            vec![turn_server_url.clone()],
            Arc::clone(&v.net0),
            agent_internal,
        )
        .await;
    }

    // Assert relay conn leak on close.
    a_agent.close().await?;
    v.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_vnet_gather_muxed_udp() -> Result<()> {
    let udp_socket = UdpSocket::bind("0.0.0.0:0").await?;
    let udp_mux = UDPMuxDefault::new(UDPMuxParams::new(udp_socket));

    let lan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "10.0.0.0/24".to_owned(),
        nat_type: Some(nat::NatType {
            mode: nat::NatMode::Nat1To1,
            ..Default::default()
        }),
        ..Default::default()
    })?));

    let nw = Arc::new(net::Net::new(Some(net::NetConfig {
        static_ips: vec!["10.0.0.1".to_owned()],
        ..Default::default()
    })));

    connect_net2router(&nw, &lan).await?;

    let a = Agent::new(AgentConfig {
        network_types: vec![NetworkType::Udp4],
        nat_1to1_ips: vec!["1.2.3.4".to_owned()],
        net: Some(nw),
        udp_network: UDPNetwork::Muxed(udp_mux),
        ..Default::default()
    })
    .await?;

    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));
    a.on_candidate(Box::new(
        move |c: Option<Arc<dyn Candidate + Send + Sync>>| {
            let done_tx_clone = Arc::clone(&done_tx);
            Box::pin(async move {
                if c.is_none() {
                    let mut tx = done_tx_clone.lock().await;
                    tx.take();
                }
            })
        },
    ));

    a.gather_candidates()?;

    log::debug!("wait for gathering is done...");
    let _ = done_rx.recv().await;
    log::debug!("gathering is done");

    let candidates = a.get_local_candidates().await?;
    assert_eq!(candidates.len(), 1, "There must be a single candidate");

    let candi = &candidates[0];
    let laddr = candi.get_conn().unwrap().local_addr()?;
    assert_eq!(candi.address(), "1.2.3.4");
    assert_eq!(
        candi.port(),
        laddr.port(),
        "Unexpected candidate port: {}",
        candi.port()
    );

    Ok(())
}
