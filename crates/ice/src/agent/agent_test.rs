use super::*;
use crate::candidate::candidate_base::*;
use crate::candidate::candidate_host::*;
use crate::candidate::candidate_peer_reflexive::*;
use crate::candidate::candidate_relay::*;
use crate::candidate::candidate_server_reflexive::*;
use crate::control::AttrControlling;
use crate::priority::PriorityAttr;

use crate::use_candidate::UseCandidateAttr;
use async_trait::async_trait;
use std::io;
use std::net::Ipv4Addr;
use std::str::FromStr;
use stun::textattrs::Username;

struct MockConn;

#[async_trait]
impl util::Conn for MockConn {
    async fn connect(&self, _addr: SocketAddr) -> io::Result<()> {
        Ok(())
    }
    async fn recv(&self, _buf: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
    async fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        Ok((0, SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0)))
    }
    async fn send(&self, _buf: &[u8]) -> io::Result<usize> {
        Ok(0)
    }
    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> io::Result<usize> {
        Ok(0)
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0))
    }
}

#[tokio::test]
async fn test_pair_search() -> Result<(), Error> {
    let config = AgentConfig::default();
    let a = Agent::new(config).await?;

    {
        let ai = a.agent_internal.lock().await;
        assert!(
            ai.checklist.is_empty(),
            "TestPairSearch is only a valid test if a.validPairs is empty on construction"
        );

        let cp = ai.get_best_available_candidate_pair();
        assert!(cp.is_none(), "No Candidate pairs should exist");
    }

    let _ = a.close(); //TODO: ?
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
    let host_local: Arc<dyn Candidate + Send + Sync> =
        Arc::new(host_config.new_candidate_host().await?);

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

    let relay_remote = relay_config.new_candidate_relay().await?;

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

    let srflx_remote = srflx_config.new_candidate_server_reflexive().await?;

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

    let prflx_remote = prflx_config.new_candidate_peer_reflexive().await?;

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
    let host_remote = host_config.new_candidate_host().await?;

    let remotes: Vec<Arc<dyn Candidate + Send + Sync>> = vec![
        Arc::new(relay_remote),
        Arc::new(srflx_remote),
        Arc::new(prflx_remote),
        Arc::new(host_remote),
    ];

    {
        let mut ai = a.agent_internal.lock().await;
        for remote in remotes {
            if ai.find_pair(&host_local, &remote).is_none() {
                ai.add_pair(host_local.clone(), remote.clone());
            }

            if let Some(p) = ai.get_pair_mut(&host_local, &remote) {
                p.state = CandidatePairState::Succeeded;
            }

            if let Some(best_pair) = ai.get_best_available_candidate_pair() {
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

    let _ = a.close(); //TODO: ?
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
    let host_local = host_config.new_candidate_host().await?;

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
    let relay_remote = relay_config.new_candidate_relay().await?;

    // select the pair
    let p = CandidatePair::new(Arc::new(host_local), Arc::new(relay_remote), false);
    {
        let mut ai = a.agent_internal.lock().await;
        ai.set_selected_pair(Some(p)).await;
    }

    // ensure that the callback fired on setting the pair
    let _ = callback_called_rx.recv().await;

    let _ = a.close(); //TODO: ?
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

    let local: Arc<dyn Candidate + Send + Sync> = Arc::new(host_config.new_candidate_host().await?);
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
        Box::new(TransactionId::default()),
        Box::new(Username::new(ATTR_USERNAME, username)),
        Box::new(UseCandidateAttr::new()),
        Box::new(AttrControlling(tie_breaker)),
        Box::new(PriorityAttr(local.priority())),
        Box::new(MessageIntegrity::new_short_term_integrity(local_pwd)),
        Box::new(FINGERPRINT),
    ])?;

    {
        let mut ai = a.agent_internal.lock().await;
        ai.handle_inbound(&mut msg, &local, remote).await;

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

    let _ = a.close(); //TODO: ?
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

    let local: Arc<dyn Candidate + Send + Sync> = Arc::new(host_config.new_candidate_host().await?);
    let remote = SocketAddr::from_str("172.17.0.3:999")?;

    let mut msg = Message::new();
    msg.build(&[
        Box::new(BINDING_SUCCESS),
        Box::new(tid),
        Box::new(MessageIntegrity::new_short_term_integrity(remote_pwd)),
        Box::new(FINGERPRINT),
    ])?;

    {
        let mut ai = a.agent_internal.lock().await;
        ai.handle_inbound(&mut msg, &local, remote).await;

        assert_eq!(
            ai.remote_candidates.len(),
            0,
            "unknown remote was able to create a candidate"
        );
    }

    let _ = a.close(); //TODO: ?
    Ok(())
}

// Assert that Agent on startup sends message, and doesn't wait for connectivityTicker to fire
#[tokio::test]
async fn test_connectivity_on_startup() -> Result<(), Error> {
    // Create a network with two interfaces
    /*TODO: wan, err := vnet.NewRouter(&vnet.RouterConfig{
        CIDR:          "0.0.0.0/0",
        LoggerFactory: logging.NewDefaultLoggerFactory(),
    })
    assert.NoError(t, err)

    net0 := vnet.NewNet(&vnet.NetConfig{
        StaticIPs: []string{"192.168.0.1"},
    })
    assert.NoError(t, wan.AddNet(net0))

    net1 := vnet.NewNet(&vnet.NetConfig{
        StaticIPs: []string{"192.168.0.2"},
    })
    assert.NoError(t, wan.AddNet(net1))

    assert.NoError(t, wan.Start())

    aNotifier, aConnected := onConnected()
    bNotifier, bConnected := onConnected()

    KeepaliveInterval := time.Hour
    cfg0 := &AgentConfig{
        NetworkTypes:     supportedNetworkTypes(),
        MulticastDNSMode: MulticastDNSModeDisabled,
        Net:              net0,

        KeepaliveInterval: &KeepaliveInterval,
        checkInterval:     time.Hour,
    }

    aAgent, err := NewAgent(cfg0)
    require.NoError(t, err)
    require.NoError(t, aAgent.OnConnectionStateChange(aNotifier))

    cfg1 := &AgentConfig{
        NetworkTypes:      supportedNetworkTypes(),
        MulticastDNSMode:  MulticastDNSModeDisabled,
        Net:               net1,
        KeepaliveInterval: &KeepaliveInterval,
        checkInterval:     time.Hour,
    }

    bAgent, err := NewAgent(cfg1)
    require.NoError(t, err)
    require.NoError(t, bAgent.OnConnectionStateChange(bNotifier))

    aConn, bConn := func(aAgent, bAgent *Agent) (*Conn, *Conn) {
        // Manual signaling
        aUfrag, aPwd, err := aAgent.GetLocalUserCredentials()
        assert.NoError(t, err)

        bUfrag, bPwd, err := bAgent.GetLocalUserCredentials()
        assert.NoError(t, err)

        gatherAndExchangeCandidates(aAgent, bAgent)

        accepted := make(chan struct{})
        accepting := make(chan struct{})
        var aConn *Conn

        origHdlr := aAgent.onConnectionStateChangeHdlr.Load()
        if origHdlr != nil {
            defer check(aAgent.OnConnectionStateChange(origHdlr.(func(ConnectionState))))
        }
        check(aAgent.OnConnectionStateChange(func(s ConnectionState) {
            if s == ConnectionStateChecking {
                close(accepting)
            }
            if origHdlr != nil {
                origHdlr.(func(ConnectionState))(s)
            }
        }))

        go func() {
            var acceptErr error
            aConn, acceptErr = aAgent.Accept(context.TODO(), bUfrag, bPwd)
            check(acceptErr)
            close(accepted)
        }()

        <-accepting

        bConn, err := bAgent.Dial(context.TODO(), aUfrag, aPwd)
        check(err)

        // Ensure accepted
        <-accepted
        return aConn, bConn
    }(aAgent, bAgent)

    // Ensure pair selected
    // Note: this assumes ConnectionStateConnected is thrown after selecting the final pair
    <-aConnected
    <-bConnected

    assert.NoError(t, wan.Stop())
    if !closePipe(t, aConn, bConn) {
        return
    }*/

    Ok(())
}

#[tokio::test]
async fn test_connectivity_lite() -> Result<(), Error> {
    /*TODO:
    stunServerURL := &URL{
        Scheme: SchemeTypeSTUN,
        Host:   "1.2.3.4",
        Port:   3478,
        Proto:  ProtoTypeUDP,
    }

    natType := &vnet.NATType{
        MappingBehavior:   vnet.EndpointIndependent,
        FilteringBehavior: vnet.EndpointIndependent,
    }
    v, err := buildVNet(natType, natType)
    require.NoError(t, err, "should succeed")
    defer v.close()

    aNotifier, aConnected := onConnected()
    bNotifier, bConnected := onConnected()

    cfg0 := &AgentConfig{
        Urls:             []*URL{stunServerURL},
        NetworkTypes:     supportedNetworkTypes(),
        MulticastDNSMode: MulticastDNSModeDisabled,
        Net:              v.net0,
    }

    aAgent, err := NewAgent(cfg0)
    require.NoError(t, err)
    require.NoError(t, aAgent.OnConnectionStateChange(aNotifier))

    cfg1 := &AgentConfig{
        Urls:             []*URL{},
        Lite:             true,
        CandidateTypes:   []CandidateType{CandidateTypeHost},
        NetworkTypes:     supportedNetworkTypes(),
        MulticastDNSMode: MulticastDNSModeDisabled,
        Net:              v.net1,
    }

    bAgent, err := NewAgent(cfg1)
    require.NoError(t, err)
    require.NoError(t, bAgent.OnConnectionStateChange(bNotifier))

    aConn, bConn := connectWithVNet(aAgent, bAgent)

    // Ensure pair selected
    // Note: this assumes ConnectionStateConnected is thrown after selecting the final pair
    <-aConnected
    <-bConnected

    if !closePipe(t, aConn, bConn) {
        return
    }*/

    Ok(())
}
