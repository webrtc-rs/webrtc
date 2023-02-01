use super::*;

use crate::candidate::candidate_host::CandidateHostConfig;
use crate::candidate::candidate_peer_reflexive::CandidatePeerReflexiveConfig;
use crate::candidate::candidate_relay::CandidateRelayConfig;
use crate::candidate::candidate_server_reflexive::CandidateServerReflexiveConfig;

pub(crate) fn host_candidate() -> Result<CandidateBase> {
    CandidateHostConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "0.0.0.0".to_owned(),
            component: COMPONENT_RTP,
            ..Default::default()
        },
        ..Default::default()
    }
    .new_candidate_host()
}

pub(crate) fn prflx_candidate() -> Result<CandidateBase> {
    CandidatePeerReflexiveConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "0.0.0.0".to_owned(),
            component: COMPONENT_RTP,
            ..Default::default()
        },
        ..Default::default()
    }
    .new_candidate_peer_reflexive()
}

pub(crate) fn srflx_candidate() -> Result<CandidateBase> {
    CandidateServerReflexiveConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "0.0.0.0".to_owned(),
            component: COMPONENT_RTP,
            ..Default::default()
        },
        ..Default::default()
    }
    .new_candidate_server_reflexive()
}

pub(crate) fn relay_candidate() -> Result<CandidateBase> {
    CandidateRelayConfig {
        base_config: CandidateBaseConfig {
            network: "udp".to_owned(),
            address: "0.0.0.0".to_owned(),
            component: COMPONENT_RTP,
            ..Default::default()
        },
        ..Default::default()
    }
    .new_candidate_relay()
}

#[test]
fn test_candidate_pair_priority() -> Result<()> {
    let tests = vec![
        (
            CandidatePair::new(
                Arc::new(host_candidate()?),
                Arc::new(host_candidate()?),
                false,
            ),
            9151314440652587007,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate()?),
                Arc::new(host_candidate()?),
                true,
            ),
            9151314440652587007,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate()?),
                Arc::new(prflx_candidate()?),
                true,
            ),
            7998392936314175488,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate()?),
                Arc::new(prflx_candidate()?),
                false,
            ),
            7998392936314175487,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate()?),
                Arc::new(srflx_candidate()?),
                true,
            ),
            7277816996102668288,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate()?),
                Arc::new(srflx_candidate()?),
                false,
            ),
            7277816996102668287,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate()?),
                Arc::new(relay_candidate()?),
                true,
            ),
            72057593987596288,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate()?),
                Arc::new(relay_candidate()?),
                false,
            ),
            72057593987596287,
        ),
    ];

    for (pair, want) in tests {
        let got = pair.priority();
        assert_eq!(
            got, want,
            "CandidatePair({pair}).Priority() = {got}, want {want}"
        );
    }

    Ok(())
}

#[test]
fn test_candidate_pair_equality() -> Result<()> {
    let pair_a = CandidatePair::new(
        Arc::new(host_candidate()?),
        Arc::new(srflx_candidate()?),
        true,
    );
    let pair_b = CandidatePair::new(
        Arc::new(host_candidate()?),
        Arc::new(srflx_candidate()?),
        false,
    );

    assert_eq!(pair_a, pair_b, "Expected {pair_a} to equal {pair_b}");

    Ok(())
}
