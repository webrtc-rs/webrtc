#[cfg(test)]
mod agent_test;

pub mod agent_config;
pub mod agent_stats;

use stun::agent::TransactionId;
//use util::Error;

use std::net::SocketAddr;

use tokio::time::Instant;

pub(crate) struct BindingRequest {
    timestamp: Instant,
    transaction_id: TransactionId,
    destination: SocketAddr,
    is_use_candidate: bool,
}
