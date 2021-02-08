use super::*;
use crate::candidate::candidate_base::CandidateBaseConfig;
use crate::candidate::candidate_host::{new_candidate_host, CandidateHostConfig};
use crate::candidate::candidate_peer_reflexive::{
    new_candidate_peer_reflexive, CandidatePeerReflexiveConfig,
};
use crate::candidate::candidate_relay::{new_candidate_relay, CandidateRelayConfig};
use crate::candidate::candidate_server_reflexive::{
    new_candidate_server_reflexive, CandidateServerReflexiveConfig,
};

#[tokio::test]
async fn test_pair_search() -> Result<(), Error> {
    let config = AgentConfig::default();
    let mut a = Agent::new(config).await?;

    assert!(
        a.checklist.is_empty(),
        "TestPairSearch is only a valid test if a.validPairs is empty on construction"
    );

    let cp = a.get_best_available_candidate_pair();
    assert!(cp.is_none(), "No Candidate pairs should exist");

    let _ = a.close(); //TODO: ?
    Ok(())
}

#[tokio::test]
async fn test_pair_priority() -> Result<(), Error> {
    let mut a = Agent::new(AgentConfig::default()).await?;

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
    let host_local: Box<dyn Candidate + Send + Sync> = Box::new(new_candidate_host(host_config)?);

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

    let relay_remote = new_candidate_relay(relay_config)?;

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

    let srflx_remote = new_candidate_server_reflexive(srflx_config)?;

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

    let prflx_remote = new_candidate_peer_reflexive(prflx_config)?;

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
    let host_remote = new_candidate_host(host_config)?;

    let remotes: Vec<Box<dyn Candidate + Send + Sync>> = vec![
        Box::new(relay_remote),
        Box::new(srflx_remote),
        Box::new(prflx_remote),
        Box::new(host_remote),
    ];

    for remote in remotes {
        if a.find_pair(&*host_local, &*remote).is_none() {
            a.add_pair(host_local.clone(), remote.clone());
        }

        if let Some(p) = a.get_pair_mut(&*host_local, &*remote) {
            p.state = CandidatePairState::Succeeded;
        }

        if let Some(best_pair) = a.get_best_available_candidate_pair() {
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

    let _ = a.close(); //TODO: ?
    Ok(())
}

#[tokio::test]
async fn test_on_selected_candidate_pair_change() -> Result<(), Error> {
    let mut a = Agent::new(AgentConfig::default()).await?;
    let (callback_called_tx, mut callback_called_rx) = mpsc::channel::<()>(1);

    // use std::sync::Mutex, instead of tokio::sync::Mutex, because of async closure is not stable yet
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
    let host_local = new_candidate_host(host_config)?;

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
    let relay_remote = new_candidate_relay(relay_config)?;

    // select the pair
    let p = CandidatePair::new(Box::new(host_local), Box::new(relay_remote), false);
    a.set_selected_pair(Some(p)).await;

    // ensure that the callback fired on setting the pair
    let _ = callback_called_rx.recv().await;

    let _ = a.close(); //TODO: ?
    Ok(())
}
