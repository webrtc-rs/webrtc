use super::agent_vnet_test::*;
use super::*;
use crate::candidate::candidate_base::*;
use crate::candidate::candidate_host::*;
use crate::candidate::candidate_peer_reflexive::*;
use crate::candidate::candidate_relay::*;
use crate::candidate::candidate_server_reflexive::*;
use crate::control::AttrControlling;
use crate::priority::PriorityAttr;
use crate::use_candidate::UseCandidateAttr;

use std::str::FromStr;
use stun::textattrs::Username;
use util::vnet::*;

#[tokio::test]
async fn test_pair_search() -> Result<(), Error> {
    let config = AgentConfig::default();
    let a = Agent::new(config).await?;

    {
        let ai = a.agent_internal.lock().await;
        {
            let checklist = ai.agent_conn.checklist.lock().await;
            assert!(
                checklist.is_empty(),
                "TestPairSearch is only a valid test if a.validPairs is empty on construction"
            );
        }

        let cp = ai.agent_conn.get_best_available_candidate_pair().await;
        assert!(cp.is_none(), "No Candidate pairs should exist");
    }

    let _ = a.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_pair_priority() -> Result<(), Error> {
    let a = Agent::new(AgentConfig::default()).await?;

    let host_config = CandidateHostConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "192.168.1.1".to_owned(),
            port: 19216,
            component: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let host_local: Arc<dyn Candidate + Send + Sync> = Arc::new(
        host_config
            .new_candidate_host(a.agent_internal.clone())
            .await?,
    );

    let relay_config = CandidateRelayConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "1.2.3.4".to_owned(),
            port: 12340,
            component: 1,
            ..Default::default()
        },
        rel_addr: "4.3.2.1".to_owned(),
        rel_port: 43210,
        ..Default::default()
    };

    let relay_remote = relay_config
        .new_candidate_relay(a.agent_internal.clone())
        .await?;

    let srflx_config = CandidateServerReflexiveConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "10.10.10.2".to_owned(),
            port: 19218,
            component: 1,
            ..Default::default()
        },
        rel_addr: "4.3.2.1".to_owned(),
        rel_port: 43212,
        ..Default::default()
    };

    let srflx_remote = srflx_config
        .new_candidate_server_reflexive(a.agent_internal.clone())
        .await?;

    let prflx_config = CandidatePeerReflexiveConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "10.10.10.2".to_owned(),
            port: 19217,
            component: 1,
            ..Default::default()
        },
        rel_addr: "4.3.2.1".to_owned(),
        rel_port: 43211,
        ..Default::default()
    };

    let prflx_remote = prflx_config
        .new_candidate_peer_reflexive(a.agent_internal.clone())
        .await?;

    let host_config = CandidateHostConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "1.2.3.5".to_owned(),
            port: 12350,
            component: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let host_remote = host_config
        .new_candidate_host(a.agent_internal.clone())
        .await?;

    let remotes: Vec<Arc<dyn Candidate + Send + Sync>> = vec![
        Arc::new(relay_remote),
        Arc::new(srflx_remote),
        Arc::new(prflx_remote),
        Arc::new(host_remote),
    ];

    {
        let mut ai = a.agent_internal.lock().await;
        for remote in remotes {
            if ai.find_pair(&host_local, &remote).await.is_none() {
                ai.add_pair(host_local.clone(), remote.clone()).await;
            }

            if let Some(p) = ai.find_pair(&host_local, &remote).await {
                p.state
                    .store(CandidatePairState::Succeeded as u8, Ordering::SeqCst);
            }

            if let Some(best_pair) = ai.agent_conn.get_best_available_candidate_pair().await {
                assert_eq!(
                    best_pair.to_string(),
                    CandidatePair {
                        remote: remote.clone(),
                        local: host_local.clone(),
                        ..Default::default()
                    }
                    .to_string(),
                    "Unexpected bestPair {} (expected remote: {})",
                    best_pair.to_string(),
                    remote.to_string(),
                );
            } else {
                assert!(false, "expected Some, but got None");
            }
        }
    }

    let _ = a.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_on_selected_candidate_pair_change() -> Result<(), Error> {
    let a = Agent::new(AgentConfig::default()).await?;
    let (callback_called_tx, mut callback_called_rx) = mpsc::channel::<()>(1);

    // use std::sync::Mutex, instead of tokio::sync::Mutex, because of async closure is not stable yet
    // DON'T mix the usage of std::sync::Mutex and tokio async in Production!!!
    let arc_tx = Arc::new(std::sync::Mutex::new(Some(callback_called_tx)));
    let cb: OnSelectedCandidatePairChangeHdlrFn = Box::new(move |_, _| {
        if let Ok(mut tx) = arc_tx.lock() {
            tx.take();
        }
    });
    a.on_selected_candidate_pair_change(cb).await;

    let host_config = CandidateHostConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "192.168.1.1".to_owned(),
            port: 19216,
            component: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let host_local = host_config
        .new_candidate_host(a.agent_internal.clone())
        .await?;

    let relay_config = CandidateRelayConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "1.2.3.4".to_owned(),
            port: 12340,
            component: 1,
            ..Default::default()
        },
        rel_addr: "4.3.2.1".to_owned(),
        rel_port: 43210,
        ..Default::default()
    };
    let relay_remote = relay_config
        .new_candidate_relay(a.agent_internal.clone())
        .await?;

    // select the pair
    let p = Arc::new(CandidatePair::new(
        Arc::new(host_local),
        Arc::new(relay_remote),
        false,
    ));
    {
        let mut ai = a.agent_internal.lock().await;
        ai.set_selected_pair(Some(p)).await;
    }

    // ensure that the callback fired on setting the pair
    let _ = callback_called_rx.recv().await;

    let _ = a.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_handle_peer_reflexive_udp_pflx_candidate() -> Result<(), Error> {
    let a = Agent::new(AgentConfig::default()).await?;

    let host_config = CandidateHostConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "192.168.0.2".to_owned(),
            port: 777,
            component: 1,
            conn: Some(Arc::new(MockConn {})),
            ..Default::default()
        },
        ..Default::default()
    };

    let local: Arc<dyn Candidate + Send + Sync> = Arc::new(
        host_config
            .new_candidate_host(a.agent_internal.clone())
            .await?,
    );
    let remote = SocketAddr::from_str("172.17.0.3:999")?;

    let (username, local_pwd, tie_breaker) = {
        let ai = a.agent_internal.lock().await;

        (
            ai.local_ufrag.to_owned() + ":" + ai.remote_ufrag.as_str(),
            ai.local_pwd.clone(),
            ai.tie_breaker,
        )
    };

    let mut msg = Message::new();
    msg.build(&[
        Box::new(BINDING_REQUEST),
        Box::new(TransactionId::new()),
        Box::new(Username::new(ATTR_USERNAME, username)),
        Box::new(UseCandidateAttr::new()),
        Box::new(AttrControlling(tie_breaker)),
        Box::new(PriorityAttr(local.priority())),
        Box::new(MessageIntegrity::new_short_term_integrity(local_pwd)),
        Box::new(FINGERPRINT),
    ])?;

    {
        let agent_internal_clone = Arc::clone(&a.agent_internal);
        let mut ai = a.agent_internal.lock().await;
        ai.handle_inbound(&mut msg, &local, remote, agent_internal_clone)
            .await;

        // length of remote candidate list must be one now
        assert_eq!(
            ai.remote_candidates.len(),
            1,
            "failed to add a network type to the remote candidate list"
        );

        // length of remote candidate list for a network type must be 1
        if let Some(cands) = ai.remote_candidates.get(&local.network_type()) {
            assert_eq!(
                cands.len(),
                1,
                "failed to add prflx candidate to remote candidate list"
            );

            let c = &cands[0];

            assert_eq!(
                c.candidate_type(),
                CandidateType::PeerReflexive,
                "candidate type must be prflx"
            );

            assert_eq!(c.address(), "172.17.0.3", "IP address mismatch");

            assert_eq!(c.port(), 999, "Port number mismatch");
        } else {
            assert!(
                false,
                "expected non-empty remote candidate for network type {}",
                local.network_type()
            );
        }
    }

    let _ = a.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_handle_peer_reflexive_unknown_remote() -> Result<(), Error> {
    let a = Agent::new(AgentConfig::default()).await?;

    let mut tid = TransactionId::default();
    tid.0[..3].copy_from_slice("ABC".as_bytes());

    let remote_pwd = {
        let mut ai = a.agent_internal.lock().await;
        ai.pending_binding_requests = vec![BindingRequest {
            timestamp: Instant::now(),
            transaction_id: tid,
            destination: SocketAddr::from_str("0.0.0.0:0")?,
            is_use_candidate: false,
        }];
        ai.remote_pwd.clone()
    };

    let host_config = CandidateHostConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "192.168.0.2".to_owned(),
            port: 777,
            component: 1,
            conn: Some(Arc::new(MockConn {})),
            ..Default::default()
        },
        ..Default::default()
    };

    let local: Arc<dyn Candidate + Send + Sync> = Arc::new(
        host_config
            .new_candidate_host(a.agent_internal.clone())
            .await?,
    );
    let remote = SocketAddr::from_str("172.17.0.3:999")?;

    let mut msg = Message::new();
    msg.build(&[
        Box::new(BINDING_SUCCESS),
        Box::new(tid),
        Box::new(MessageIntegrity::new_short_term_integrity(remote_pwd)),
        Box::new(FINGERPRINT),
    ])?;

    {
        let agent_internal_clone = Arc::clone(&a.agent_internal);
        let mut ai = a.agent_internal.lock().await;
        ai.handle_inbound(&mut msg, &local, remote, agent_internal_clone)
            .await;

        assert_eq!(
            ai.remote_candidates.len(),
            0,
            "unknown remote was able to create a candidate"
        );
    }

    let _ = a.close().await?;
    Ok(())
}

//use std::io::Write;

// Assert that Agent on startup sends message, and doesn't wait for connectivityTicker to fire
#[tokio::test]
async fn test_connectivity_on_startup() -> Result<(), Error> {
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
    let wan = Arc::new(Mutex::new(router::Router::new(router::RouterConfig {
        cidr: "0.0.0.0/0".to_owned(),
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

    connect_net2router(&net0, &wan).await?;
    connect_net2router(&net1, &wan).await?;
    start_router(&wan).await?;

    let (a_notifier, mut a_connected) = on_connected();
    let (b_notifier, mut b_connected) = on_connected();

    let keepalive_interval = Some(Duration::from_secs(3600)); //time.Hour
    let check_interval = Duration::from_secs(3600); //time.Hour
    let cfg0 = AgentConfig {
        network_types: supported_network_types(),
        multicast_dns_mode: MulticastDnsMode::Disabled,
        net: Some(net0),

        keepalive_interval,
        check_interval,
        ..Default::default()
    };

    let a_agent = Arc::new(Agent::new(cfg0).await?);
    a_agent.on_connection_state_change(a_notifier).await;

    let cfg1 = AgentConfig {
        network_types: supported_network_types(),
        multicast_dns_mode: MulticastDnsMode::Disabled,
        net: Some(net1),

        keepalive_interval,
        check_interval,
        ..Default::default()
    };

    let b_agent = Arc::new(Agent::new(cfg1).await?);
    b_agent.on_connection_state_change(b_notifier).await;

    // Manual signaling
    let (a_ufrag, a_pwd) = a_agent.get_local_user_credentials().await;
    let (b_ufrag, b_pwd) = b_agent.get_local_user_credentials().await;

    gather_and_exchange_candidates(&a_agent, &b_agent).await?;

    let (accepted_tx, mut accepted_rx) = mpsc::channel::<()>(1);
    let (accepting_tx, mut accepting_rx) = mpsc::channel::<()>(1);

    let mut accepting_tx = Some(accepting_tx);
    //origHdlr := a_agent.onConnectionStateChangeHdlr.Load()
    //if origHdlr != nil {
    //    defer check(a_agent.OnConnectionStateChange(origHdlr.(func(ConnectionState))))
    //}
    a_agent
        .on_connection_state_change(Box::new(move |s: ConnectionState| {
            if s == ConnectionState::Checking {
                accepting_tx.take();
            }
            //if origHdlr != nil {
            //    origHdlr.(func(ConnectionState))(s)
            //}
        }))
        .await;

    tokio::spawn(async move {
        let result = a_agent.accept(b_ufrag, b_pwd).await;
        assert!(result.is_ok(), "agent accept expected OK");
        drop(accepted_tx);
    });

    let _ = accepting_rx.recv().await;

    let _ = b_agent.dial(a_ufrag, a_pwd).await?;

    // Ensure accepted
    let _ = accepted_rx.recv().await;

    // Ensure pair selected
    // Note: this assumes ConnectionStateConnected is thrown after selecting the final pair
    let _ = a_connected.recv().await;
    let _ = b_connected.recv().await;

    {
        let mut w = wan.lock().await;
        w.stop().await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_connectivity_lite() -> Result<(), Error> {
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
        host: "1.2.3.4".to_owned(),
        port: 3478,
        proto: ProtoType::Udp,
        ..Default::default()
    };

    let nat_type = nat::NATType {
        mapping_behavior: nat::EndpointDependencyType::EndpointIndependent,
        filtering_behavior: nat::EndpointDependencyType::EndpointIndependent,
        ..Default::default()
    };

    let v = build_vnet(nat_type, nat_type).await?;

    let (a_notifier, mut a_connected) = on_connected();
    let (b_notifier, mut b_connected) = on_connected();

    let cfg0 = AgentConfig {
        urls: vec![stun_server_url],
        network_types: supported_network_types(),
        multicast_dns_mode: MulticastDnsMode::Disabled,
        net: Some(Arc::clone(&v.net0)),
        ..Default::default()
    };

    let a_agent = Arc::new(Agent::new(cfg0).await?);
    a_agent.on_connection_state_change(a_notifier).await;

    let cfg1 = AgentConfig {
        urls: vec![],
        lite: true,
        candidate_types: vec![CandidateType::Host],
        network_types: supported_network_types(),
        multicast_dns_mode: MulticastDnsMode::Disabled,
        net: Some(Arc::clone(&v.net1)),
        ..Default::default()
    };

    let b_agent = Arc::new(Agent::new(cfg1).await?);
    b_agent.on_connection_state_change(b_notifier).await;

    let _ = connect_with_vnet(&a_agent, &b_agent).await?;

    // Ensure pair selected
    // Note: this assumes ConnectionStateConnected is thrown after selecting the final pair
    let _ = a_connected.recv().await;
    let _ = b_connected.recv().await;

    v.close().await?;

    Ok(())
}
