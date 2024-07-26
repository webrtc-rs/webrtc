use portable_atomic::{AtomicU16, AtomicU8};
use std::sync::Arc;

use util::sync::Mutex as SyncMutex;

use super::candidate_base::*;
use super::*;
use crate::error::*;
use crate::rand::generate_cand_id;
use crate::util::*;

/// The config required to create a new `CandidateRelay`.
#[derive(Default)]
pub struct CandidateRelayConfig {
    pub base_config: CandidateBaseConfig,

    pub rel_addr: String,
    pub rel_port: u16,
    pub relay_client: Option<Arc<turn::client::Client>>,
}

impl CandidateRelayConfig {
    /// Creates a new relay candidate.
    pub fn new_candidate_relay(self) -> Result<CandidateBase> {
        let mut candidate_id = self.base_config.candidate_id;
        if candidate_id.is_empty() {
            candidate_id = generate_cand_id();
        }

        let ip: IpAddr = match self.base_config.address.parse() {
            Ok(ip) => ip,
            Err(_) => return Err(Error::ErrAddressParseFailed),
        };
        let network_type = determine_network_type(&self.base_config.network, &ip)?;

        let c = CandidateBase {
            id: candidate_id,
            network_type: AtomicU8::new(network_type as u8),
            candidate_type: CandidateType::Relay,
            address: self.base_config.address,
            port: self.base_config.port,
            resolved_addr: SyncMutex::new(create_addr(network_type, ip, self.base_config.port)),
            component: AtomicU16::new(self.base_config.component),
            foundation_override: self.base_config.foundation,
            priority_override: self.base_config.priority,
            related_address: Some(CandidateRelatedAddress {
                address: self.rel_addr,
                port: self.rel_port,
            }),
            conn: self.base_config.conn,
            relay_client: self.relay_client.clone(),
            ..CandidateBase::default()
        };

        Ok(c)
    }
}
