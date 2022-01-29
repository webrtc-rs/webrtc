use std::fmt;
use std::sync::Arc;

use ice::candidate::candidate_base::CandidateBaseConfig;
use ice::candidate::candidate_host::CandidateHostConfig;
use ice::candidate::candidate_peer_reflexive::CandidatePeerReflexiveConfig;
use ice::candidate::candidate_relay::CandidateRelayConfig;
use ice::candidate::candidate_server_reflexive::CandidateServerReflexiveConfig;
use ice::candidate::Candidate;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::ice_transport::ice_candidate_type::RTCIceCandidateType;
use crate::ice_transport::ice_protocol::RTCIceProtocol;

/// ICECandidate represents a ice candidate
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RTCIceCandidate {
    pub stats_id: String,
    pub foundation: String,
    pub priority: u32,
    pub address: String,
    pub protocol: RTCIceProtocol,
    pub port: u16,
    pub typ: RTCIceCandidateType,
    pub component: u16,
    pub related_address: String,
    pub related_port: u16,
    pub tcp_type: String,
}

/// Conversion for ice_candidates
pub(crate) fn rtc_ice_candidates_from_ice_candidates(
    ice_candidates: &[Arc<dyn Candidate + Send + Sync>],
) -> Vec<RTCIceCandidate> {
    ice_candidates.iter().map(|c| c.into()).collect()
}

impl From<&Arc<dyn Candidate + Send + Sync>> for RTCIceCandidate {
    fn from(c: &Arc<dyn Candidate + Send + Sync>) -> Self {
        let typ: RTCIceCandidateType = c.candidate_type().into();
        let protocol = RTCIceProtocol::from(c.network_type().network_short().as_str());
        let (related_address, related_port) = if let Some(ra) = c.related_address() {
            (ra.address, ra.port)
        } else {
            (String::new(), 0)
        };

        RTCIceCandidate {
            stats_id: c.id(),
            foundation: c.foundation(),
            priority: c.priority(),
            address: c.address(),
            protocol,
            port: c.port(),
            component: c.component(),
            typ,
            tcp_type: c.tcp_type().to_string(),
            related_address,
            related_port,
        }
    }
}

impl RTCIceCandidate {
    pub(crate) async fn to_ice(&self) -> Result<impl Candidate> {
        let candidate_id = self.stats_id.clone();
        let c = match self.typ {
            RTCIceCandidateType::Host => {
                let config = CandidateHostConfig {
                    base_config: CandidateBaseConfig {
                        candidate_id,
                        network: self.protocol.to_string(),
                        address: self.address.clone(),
                        port: self.port,
                        component: self.component,
                        //tcp_type: ice.NewTCPType(c.TCPType),
                        foundation: self.foundation.clone(),
                        priority: self.priority,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                config.new_candidate_host().await?
            }
            RTCIceCandidateType::Srflx => {
                let config = CandidateServerReflexiveConfig {
                    base_config: CandidateBaseConfig {
                        candidate_id,
                        network: self.protocol.to_string(),
                        address: self.address.clone(),
                        port: self.port,
                        component: self.component,
                        foundation: self.foundation.clone(),
                        priority: self.priority,
                        ..Default::default()
                    },
                    rel_addr: self.related_address.clone(),
                    rel_port: self.related_port,
                };
                config.new_candidate_server_reflexive().await?
            }
            RTCIceCandidateType::Prflx => {
                let config = CandidatePeerReflexiveConfig {
                    base_config: CandidateBaseConfig {
                        candidate_id,
                        network: self.protocol.to_string(),
                        address: self.address.clone(),
                        port: self.port,
                        component: self.component,
                        foundation: self.foundation.clone(),
                        priority: self.priority,
                        ..Default::default()
                    },
                    rel_addr: self.related_address.clone(),
                    rel_port: self.related_port,
                };
                config.new_candidate_peer_reflexive().await?
            }
            RTCIceCandidateType::Relay => {
                let config = CandidateRelayConfig {
                    base_config: CandidateBaseConfig {
                        candidate_id,
                        network: self.protocol.to_string(),
                        address: self.address.clone(),
                        port: self.port,
                        component: self.component,
                        foundation: self.foundation.clone(),
                        priority: self.priority,
                        ..Default::default()
                    },
                    rel_addr: self.related_address.clone(),
                    rel_port: self.related_port,
                    relay_client: None, //TODO?
                };
                config.new_candidate_relay().await?
            }
            _ => return Err(Error::ErrICECandidateTypeUnknown),
        };

        Ok(c)
    }

    /// to_json returns an ICECandidateInit
    /// as indicated by the spec <https://w3c.github.io/webrtc-pc/#dom-rtcicecandidate-tojson>
    pub async fn to_json(&self) -> Result<RTCIceCandidateInit> {
        let candidate = self.to_ice().await?;

        Ok(RTCIceCandidateInit {
            candidate: format!("candidate:{}", candidate.marshal()),
            sdp_mid: Some("".to_owned()),
            sdp_mline_index: Some(0u16),
            username_fragment: None,
        })
    }
}

impl fmt::Display for RTCIceCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}:{}{}",
            self.protocol, self.typ, self.address, self.port, self.related_address,
        )
    }
}

/// ICECandidateInit is used to serialize ice candidates
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RTCIceCandidateInit {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    #[serde(rename = "sdpMLineIndex")]
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}
