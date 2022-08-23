use super::*;

use crate::candidate::candidate_host::CandidateHostConfig;
use crate::candidate::candidate_peer_reflexive::CandidatePeerReflexiveConfig;
use crate::candidate::candidate_relay::CandidateRelayConfig;
use crate::candidate::candidate_server_reflexive::CandidateServerReflexiveConfig;

pub(crate) async fn host_candidate() -> Result<CandidateBase> {
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
    .await
}

pub(crate) async fn prflx_candidate() -> Result<CandidateBase> {
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
    .await
}

pub(crate) async fn srflx_candidate() -> Result<CandidateBase> {
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
    .await
}

pub(crate) async fn relay_candidate() -> Result<CandidateBase> {
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
    .await
}

#[tokio::test]
async fn test_candidate_pair_priority() -> Result<()> {
    let tests = vec![
        (
            CandidatePair::new(
                Arc::new(host_candidate().await?),
                Arc::new(host_candidate().await?),
                false,
            ),
            9151314440652587007,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate().await?),
                Arc::new(host_candidate().await?),
                true,
            ),
            9151314440652587007,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate().await?),
                Arc::new(prflx_candidate().await?),
                true,
            ),
            7998392936314175488,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate().await?),
                Arc::new(prflx_candidate().await?),
                false,
            ),
            7998392936314175487,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate().await?),
                Arc::new(srflx_candidate().await?),
                true,
            ),
            7277816996102668288,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate().await?),
                Arc::new(srflx_candidate().await?),
                false,
            ),
            7277816996102668287,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate().await?),
                Arc::new(relay_candidate().await?),
                true,
            ),
            72057593987596288,
        ),
        (
            CandidatePair::new(
                Arc::new(host_candidate().await?),
                Arc::new(relay_candidate().await?),
                false,
            ),
            72057593987596287,
        ),
    ];

    for (pair, want) in tests {
        let got = pair.priority();
        assert_eq!(
            got, want,
            "CandidatePair({}).Priority() = {}, want {}",
            pair, got, want
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_candidate_pair_equality() -> Result<()> {
    let pair_a = CandidatePair::new(
        Arc::new(host_candidate().await?),
        Arc::new(srflx_candidate().await?),
        true,
    );
    let pair_b = CandidatePair::new(
        Arc::new(host_candidate().await?),
        Arc::new(srflx_candidate().await?),
        false,
    );

    assert_eq!(pair_a, pair_b, "Expected {} to equal {}", pair_a, pair_b);

    Ok(())
}
