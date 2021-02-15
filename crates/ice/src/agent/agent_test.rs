use super::*;
use crate::candidate::candidate_base::*;
use crate::candidate::candidate_host::*;
use crate::candidate::candidate_peer_reflexive::*;
use crate::candidate::candidate_relay::*;
use crate::candidate::candidate_server_reflexive::*;

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
    let host_local: Arc<dyn Candidate + Send + Sync> = Arc::new(host_config.new_candidate_host()?);

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

    let relay_remote = relay_config.new_candidate_relay()?;

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

    let srflx_remote = srflx_config.new_candidate_server_reflexive()?;

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

    let prflx_remote = prflx_config.new_candidate_peer_reflexive()?;

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
    let host_remote = host_config.new_candidate_host()?;

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
    let host_local = host_config.new_candidate_host()?;

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
    let relay_remote = relay_config.new_candidate_relay()?;

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

    /*a.selector = ControllingSelector{agent: a, log: a.log}

    hostConfig := CandidateHostConfig{
        Network:   "udp",
        Address:   "192.168.0.2",
        Port:      777,
        Component: 1,
    }
    local, err := NewCandidateHost(&hostConfig)
    local.conn = &mockPacketConn{}
    if err != nil {
        t.Fatalf("failed to create a new candidate: %v", err)
    }

    remote := &net.UDPAddr{IP: net.ParseIP("172.17.0.3"), Port: 999}

    msg, err := stun.Build(stun.BindingRequest, stun.TransactionID,
        stun.NewUsername(a.localUfrag+":"+a.remoteUfrag),
        UseCandidate(),
        AttrControlling(a.tieBreaker),
        PriorityAttr(local.Priority()),
        stun.NewShortTermIntegrity(a.localPwd),
        stun.Fingerprint,
    )
    if err != nil {
        t.Fatal(err)
    }

    a.handleInbound(msg, local, remote)

    // length of remote candidate list must be one now
    if len(a.remoteCandidates) != 1 {
        t.Fatal("failed to add a network type to the remote candidate list")
    }

    // length of remote candidate list for a network type must be 1
    set := a.remoteCandidates[local.NetworkType()]
    if len(set) != 1 {
        t.Fatal("failed to add prflx candidate to remote candidate list")
    }

    c := set[0]

    if c.Type() != CandidateTypePeerReflexive {
        t.Fatal("candidate type must be prflx")
    }

    if c.Address() != "172.17.0.3" {
        t.Fatal("IP address mismatch")
    }

    if c.Port() != 999 {
        t.Fatal("Port number mismatch")
    }*/

    let _ = a.close(); //TODO: ?
    Ok(())
}

#[tokio::test]
async fn test_handle_peer_reflexive_bad_network_type() -> Result<(), Error> {
    let a = Agent::new(AgentConfig::default()).await?;

    let _ = a.close(); //TODO: ?
    Ok(())
}

#[tokio::test]
async fn test_handle_peer_reflexive_unknown_remote() -> Result<(), Error> {
    let a = Agent::new(AgentConfig::default()).await?;

    let _ = a.close(); //TODO: ?
    Ok(())
}
