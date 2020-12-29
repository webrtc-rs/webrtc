use super::candidate_base::*;
use super::*;
use crate::errors::*;

// CandidateHostConfig is the config required to create a new CandidateHost
pub struct CandidateHostConfig {
    pub base_config: CandidateBaseConfig,

    pub tcp_type: TCPType,
}

// NewCandidateHost creates a new host candidate
pub fn new_candidate_host(config: CandidateHostConfig) -> Result<Box<dyn Candidate>, Error> {
    let candidate_id = config.base_config.candidate_id;
    /*TODO:
    if candidateID == "" {
        candidateID = globalCandidateIDGenerator.Generate()
    }*/

    let mut c = CandidateBase {
        id: candidate_id,
        address: config.base_config.address.clone(),
        candidate_type: CandidateType::Host,
        component: config.base_config.component,
        port: config.base_config.port,
        tcp_type: config.tcp_type,
        foundation_override: config.base_config.foundation,
        priority_override: config.base_config.priority,
        network: config.base_config.network,
        ..Default::default()
    };

    if !config.base_config.address.ends_with(".local") {
        match config.base_config.address.parse() {
            Ok(ip) => c.set_ip(&ip)?,
            Err(_) => return Err(ERR_ADDRESS_PARSE_FAILED.to_owned()),
        }
    } else {
        // Until mDNS candidate is resolved assume it is UDPv4
        c.network_type = NetworkType::UDP4;
    }

    Ok(Box::new(c))
}
