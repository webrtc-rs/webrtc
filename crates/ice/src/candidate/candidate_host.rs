use super::candidate_base::*;
use super::*;
use crate::errors::*;
use crate::rand::generate_cand_id;
use std::sync::atomic::{AtomicU16, AtomicU8};
use std::sync::Arc;

// CandidateHostConfig is the config required to create a new CandidateHost
#[derive(Default)]
pub struct CandidateHostConfig {
    pub base_config: CandidateBaseConfig,

    pub tcp_type: TCPType,
}

impl CandidateHostConfig {
    // NewCandidateHost creates a new host candidate
    pub fn new_candidate_host(self) -> Result<CandidateBase, Error> {
        let mut candidate_id = self.base_config.candidate_id;
        if candidate_id.is_empty() {
            candidate_id = generate_cand_id();
        }

        let (network_type, resolved_addr) = if !self.base_config.address.ends_with(".local") {
            match self.base_config.address.parse() {
                Ok(ip) => get_ip(&self.base_config.network, &ip, self.base_config.port),
                Err(_) => return Err(ERR_ADDRESS_PARSE_FAILED.to_owned()),
            }
        } else {
            // Until mDNS candidate is resolved assume it is UDPv4
            (
                NetworkType::UDP4,
                SocketAddr::new(IpAddr::from([0, 0, 0, 0]), 0),
            )
        };

        Ok(CandidateBase {
            id: candidate_id,
            address: self.base_config.address.clone(),
            candidate_type: CandidateType::Host,
            component: Arc::new(AtomicU16::new(self.base_config.component)),
            port: self.base_config.port,
            tcp_type: self.tcp_type,
            foundation_override: self.base_config.foundation,
            priority_override: self.base_config.priority,
            network: self.base_config.network,
            network_type: Arc::new(AtomicU8::new(network_type as u8)),
            resolved_addr,
            conn: self.base_config.conn,
            ..Default::default()
        })
    }
}
