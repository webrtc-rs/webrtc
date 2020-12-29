use super::candidate_base::*;
use super::*;
use crate::errors::*;
use crate::util::*;

// CandidateServerReflexiveConfig is the config required to create a new CandidateServerReflexive
pub struct CandidateServerReflexiveConfig {
    pub base_config: CandidateBaseConfig,

    pub rel_addr: String,
    pub rel_port: u16,
}

// new_candidate_server_reflexive creates a new server reflective candidate
pub fn new_candidate_server_reflexive(
    config: CandidateServerReflexiveConfig,
) -> Result<Box<dyn Candidate>, Error> {
    let ip: IpAddr = match config.base_config.address.parse() {
        Ok(ip) => ip,
        Err(_) => return Err(ERR_ADDRESS_PARSE_FAILED.to_owned()),
    };
    let network_type = determine_network_type(&config.base_config.network, &ip)?;

    let candidate_id = config.base_config.candidate_id;
    /*TODO: if candidateID == "" {
        candidateID = globalCandidateIDGenerator.Generate()
    }*/

    Ok(Box::new(CandidateBase {
        id: candidate_id,
        network_type,
        candidate_type: CandidateType::ServerReflexive,
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
        ..Default::default()
    }))
}
