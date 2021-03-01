use super::candidate_base::*;
use super::*;
use crate::errors::*;
use crate::rand::generate_cand_id;
use crate::util::*;
use std::sync::atomic::{AtomicU16, AtomicU8};
use std::sync::Arc;

// CandidateRelayConfig is the config required to create a new CandidateRelay
#[derive(Default)]
pub struct CandidateRelayConfig {
    pub base_config: CandidateBaseConfig,

    pub rel_addr: String,
    pub rel_port: u16,
    pub relay_client: Option<Arc<turn::client::Client>>,
}

impl CandidateRelayConfig {
    // new_candidate_relay creates a new relay candidate
    pub async fn new_candidate_relay(self) -> Result<CandidateBase, Error> {
        let mut candidate_id = self.base_config.candidate_id;
        if candidate_id.is_empty() {
            candidate_id = generate_cand_id();
        }

        let ip: IpAddr = match self.base_config.address.parse() {
            Ok(ip) => ip,
            Err(_) => return Err(ERR_ADDRESS_PARSE_FAILED.to_owned()),
        };
        let network_type = determine_network_type(&self.base_config.network, &ip)?;

        let c = CandidateBase {
            id: candidate_id,
            network_type: Arc::new(AtomicU8::new(network_type as u8)),
            candidate_type: CandidateType::Relay,
            address: self.base_config.address,
            port: self.base_config.port,
            resolved_addr: create_addr(network_type, ip, self.base_config.port),
            component: Arc::new(AtomicU16::new(self.base_config.component)),
            foundation_override: self.base_config.foundation,
            priority_override: self.base_config.priority,
            related_address: Some(CandidateRelatedAddress {
                address: self.rel_addr,
                port: self.rel_port,
            }),
            conn: self.base_config.conn,
            relay_client: self.relay_client.clone(),
            ..Default::default()
        };

        Ok(c)
    }
}
