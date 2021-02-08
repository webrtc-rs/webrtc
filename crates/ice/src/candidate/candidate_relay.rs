use super::candidate_base::*;
use super::*;
use crate::errors::*;
use crate::rand::generate_cand_id;
use crate::util::*;

// CandidateRelayConfig is the config required to create a new CandidateRelay
#[derive(Debug, Default)]
pub struct CandidateRelayConfig {
    pub base_config: CandidateBaseConfig,

    pub rel_addr: String,
    pub rel_port: u16,
    pub on_close: Option<OnClose>,
}

// new_candidate_relay creates a new relay candidate
pub fn new_candidate_relay(config: CandidateRelayConfig) -> Result<CandidateBase, Error> {
    let mut candidate_id = config.base_config.candidate_id;
    if candidate_id.is_empty() {
        candidate_id = generate_cand_id();
    }

    let ip: IpAddr = match config.base_config.address.parse() {
        Ok(ip) => ip,
        Err(_) => return Err(ERR_ADDRESS_PARSE_FAILED.to_owned()),
    };
    let network_type = determine_network_type(&config.base_config.network, &ip)?;

    Ok(CandidateBase {
        id: candidate_id,
        network_type,
        candidate_type: CandidateType::Relay,
        address: config.base_config.address,
        port: config.base_config.port,
        resolved_addr: create_addr(network_type, ip, config.base_config.port),
        component: config.base_config.component,
        foundation_override: config.base_config.foundation,
        priority_override: config.base_config.priority,
        related_address: Some(CandidateRelatedAddress {
            address: config.rel_addr,
            port: config.rel_port,
        }),
        on_close: config.on_close,
        ..Default::default()
    })
}
