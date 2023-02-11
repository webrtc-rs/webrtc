use regex::Regex;
use tokio::sync::{mpsc, Mutex};

use super::*;
use crate::agent::agent_config::*;
use crate::agent::agent_vnet_test::*;
use crate::agent::*;
use crate::candidate::*;
use crate::error::Error;
use crate::network_type::*;

#[tokio::test]
// This test is disabled on Windows for now because it gets stuck and never finishes.
// This does not seem to have happened due to a code change. It started happening with
// `ce55c3a066ab461c3e74f0d5ac6f1209205e79bc` but was verified as happening on
// `92cc698a3dc6da459f3bf3789fd046c2dffdf107` too.
#[cfg(not(windows))]
async fn test_multicast_dns_only_connection() -> Result<()> {
    let cfg0 = AgentConfig {
        network_types: vec![NetworkType::Udp4],
        candidate_types: vec![CandidateType::Host],
        multicast_dns_mode: MulticastDnsMode::QueryAndGather,
        ..Default::default()
    };

    let a_agent = Arc::new(Agent::new(cfg0).await?);
    let (a_notifier, mut a_connected) = on_connected();
    a_agent.on_connection_state_change(a_notifier);

    let cfg1 = AgentConfig {
        network_types: vec![NetworkType::Udp4],
        candidate_types: vec![CandidateType::Host],
        multicast_dns_mode: MulticastDnsMode::QueryAndGather,
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

    Ok(())
}

#[tokio::test]
async fn test_multicast_dns_mixed_connection() -> Result<()> {
    let cfg0 = AgentConfig {
        network_types: vec![NetworkType::Udp4],
        candidate_types: vec![CandidateType::Host],
        multicast_dns_mode: MulticastDnsMode::QueryAndGather,
        ..Default::default()
    };

    let a_agent = Arc::new(Agent::new(cfg0).await?);
    let (a_notifier, mut a_connected) = on_connected();
    a_agent.on_connection_state_change(a_notifier);

    let cfg1 = AgentConfig {
        network_types: vec![NetworkType::Udp4],
        candidate_types: vec![CandidateType::Host],
        multicast_dns_mode: MulticastDnsMode::QueryOnly,
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

    Ok(())
}

#[tokio::test]
async fn test_multicast_dns_static_host_name() -> Result<()> {
    let cfg0 = AgentConfig {
        network_types: vec![NetworkType::Udp4],
        candidate_types: vec![CandidateType::Host],
        multicast_dns_mode: MulticastDnsMode::QueryAndGather,
        multicast_dns_host_name: "invalidHostName".to_owned(),
        ..Default::default()
    };
    if let Err(err) = Agent::new(cfg0).await {
        assert_eq!(err, Error::ErrInvalidMulticastDnshostName);
    } else {
        panic!("expected error, but got ok");
    }

    let cfg1 = AgentConfig {
        network_types: vec![NetworkType::Udp4],
        candidate_types: vec![CandidateType::Host],
        multicast_dns_mode: MulticastDnsMode::QueryAndGather,
        multicast_dns_host_name: "validName.local".to_owned(),
        ..Default::default()
    };

    let a = Agent::new(cfg1).await?;

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

    Ok(())
}

#[test]
fn test_generate_multicast_dnsname() -> Result<()> {
    let name = generate_multicast_dns_name();

    let re = Regex::new(
        r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}.local+$",
    );

    if let Ok(re) = re {
        assert!(
            re.is_match(&name),
            "mDNS name must be UUID v4 + \".local\" suffix, got {name}"
        );
    } else {
        panic!("expected ok, but got err");
    }

    Ok(())
}
