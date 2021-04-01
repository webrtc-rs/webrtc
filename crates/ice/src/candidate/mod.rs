#[cfg(test)]
mod candidate_pair_test;

pub mod candidate_base;
pub mod candidate_host;
pub mod candidate_peer_reflexive;
pub mod candidate_relay;
pub mod candidate_server_reflexive;

use crate::errors::*;
use crate::network_type::*;
use crate::tcp_type::*;
use candidate_base::*;
use candidate_host::*;
use candidate_peer_reflexive::*;
use candidate_relay::*;
use candidate_server_reflexive::*;

use util::Error;

use crate::agent::agent_internal::AgentInternal;
use async_trait::async_trait;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{broadcast, Mutex};

pub(crate) const RECEIVE_MTU: usize = 8192;
pub(crate) const DEFAULT_LOCAL_PREFERENCE: u16 = 65535;

// COMPONENT_RTP indicates that the candidate is used for RTP
pub(crate) const COMPONENT_RTP: u16 = 1;
// COMPONENT_RTCP indicates that the candidate is used for RTCP
pub(crate) const COMPONENT_RTCP: u16 = 0;

// Candidate represents an ICE candidate
#[async_trait]
pub trait Candidate: fmt::Display {
    // An arbitrary string used in the freezing algorithm to
    // group similar candidates.  It is the same for two candidates that
    // have the same type, base IP address, protocol (UDP, TCP, etc.),
    // and STUN or TURN server.
    fn foundation(&self) -> String;

    // ID is a unique identifier for just this candidate
    // Unlike the foundation this is different for each candidate
    fn id(&self) -> String;

    // A component is a piece of a data stream.
    // An example is one for RTP, and one for RTCP
    fn component(&self) -> u16;
    fn set_component(&self, c: u16);

    // The last time this candidate received traffic
    fn last_received(&self) -> SystemTime;

    // The last time this candidate sent traffic
    fn last_sent(&self) -> SystemTime;

    fn network_type(&self) -> NetworkType;
    fn address(&self) -> String;
    fn port(&self) -> u16;

    fn priority(&self) -> u32;

    // A transport address related to a
    //  candidate, which is useful for diagnostics and other purposes
    fn related_address(&self) -> Option<CandidateRelatedAddress>;

    fn candidate_type(&self) -> CandidateType;
    fn tcp_type(&self) -> TcpType;

    fn marshal(&self) -> String;

    async fn addr(&self) -> SocketAddr;

    async fn close(&self) -> Result<(), Error>;
    fn seen(&self, outbound: bool);

    async fn write_to(
        &self,
        raw: &[u8],
        dst: &(dyn Candidate + Send + Sync),
    ) -> Result<usize, Error>;
    fn equal(&self, other: &dyn Candidate) -> bool;
    fn clone(&self) -> Arc<dyn Candidate + Send + Sync>;
    async fn set_ip(&self, ip: &IpAddr) -> Result<(), Error>;
    fn get_conn(&self) -> Option<&Arc<dyn util::Conn + Send + Sync>>;
    fn get_agent(&self) -> Option<&Arc<Mutex<AgentInternal>>>;
    fn get_closed_ch(&self) -> Arc<Mutex<Option<broadcast::Sender<()>>>>;
}

// CandidateType represents the type of candidate
// CandidateType enum
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum CandidateType {
    Unspecified,
    Host,
    ServerReflexive,
    PeerReflexive,
    Relay,
}

// String makes CandidateType printable
impl fmt::Display for CandidateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            CandidateType::Host => "host",
            CandidateType::ServerReflexive => "srflx",
            CandidateType::PeerReflexive => "prflx",
            CandidateType::Relay => "relay",
            CandidateType::Unspecified => "Unknown candidate type",
        };
        write!(f, "{}", s)
    }
}

impl Default for CandidateType {
    fn default() -> Self {
        CandidateType::Unspecified
    }
}

impl CandidateType {
    // preference returns the preference weight of a CandidateType
    //
    // 4.1.2.2.  Guidelines for Choosing Type and Local Preferences
    // The RECOMMENDED values are 126 for host candidates, 100
    // for server reflexive candidates, 110 for peer reflexive candidates,
    // and 0 for relayed candidates.
    pub fn preference(&self) -> u16 {
        match *self {
            CandidateType::Host => 126,
            CandidateType::PeerReflexive => 110,
            CandidateType::ServerReflexive => 100,
            CandidateType::Relay | CandidateType::Unspecified => 0,
        }
    }
}

pub(crate) fn contains_candidate_type(
    candidate_type: CandidateType,
    candidate_type_list: &[CandidateType],
) -> bool {
    if candidate_type_list.is_empty() {
        return false;
    }
    for ct in candidate_type_list {
        if *ct == candidate_type {
            return true;
        }
    }
    false
}

// CandidateRelatedAddress convey transport addresses related to the
// candidate, useful for diagnostics and other purposes.
#[derive(PartialEq, Debug, Clone)]
pub struct CandidateRelatedAddress {
    pub address: String,
    pub port: u16,
}

// String makes CandidateRelatedAddress printable
impl fmt::Display for CandidateRelatedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, " related {}:{}", self.address, self.port)
    }
}

// CandidatePairState represent the ICE candidate pair state
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum CandidatePairState {
    Unspecified = 0,

    // CandidatePairStateWaiting means a check has not been performed for
    // this pair
    Waiting = 1,

    // CandidatePairStateInProgress means a check has been sent for this pair,
    // but the transaction is in progress.
    InProgress = 2,

    // CandidatePairStateFailed means a check for this pair was already done
    // and failed, either never producing any response or producing an unrecoverable
    // failure response.
    Failed = 3,

    // CandidatePairStateSucceeded means a check for this pair was already
    // done and produced a successful result.
    Succeeded = 4,
}

impl From<u8> for CandidatePairState {
    fn from(v: u8) -> Self {
        match v {
            1 => CandidatePairState::Waiting,
            2 => CandidatePairState::InProgress,
            3 => CandidatePairState::Failed,
            4 => CandidatePairState::Succeeded,
            _ => CandidatePairState::Unspecified,
        }
    }
}

impl Default for CandidatePairState {
    fn default() -> Self {
        CandidatePairState::Unspecified
    }
}

impl fmt::Display for CandidatePairState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            CandidatePairState::Waiting => "waiting",
            CandidatePairState::InProgress => "in-progress",
            CandidatePairState::Failed => "failed",
            CandidatePairState::Succeeded => "succeeded",
            CandidatePairState::Unspecified => "unspecified",
        };

        write!(f, "{}", s)
    }
}

// candidatePair represents a combination of a local and remote candidate
pub(crate) struct CandidatePair {
    pub(crate) ice_role_controlling: AtomicBool,
    pub(crate) remote: Arc<dyn Candidate + Send + Sync>,
    pub(crate) local: Arc<dyn Candidate + Send + Sync>,
    pub(crate) binding_request_count: AtomicU16,
    pub(crate) state: AtomicU8, // convert it to CandidatePairState,
    pub(crate) nominated: AtomicBool,
}

impl Default for CandidatePair {
    fn default() -> Self {
        CandidatePair {
            ice_role_controlling: AtomicBool::new(false),
            remote: Arc::new(CandidateBase::default()),
            local: Arc::new(CandidateBase::default()),
            state: AtomicU8::new(CandidatePairState::Waiting as u8),
            binding_request_count: AtomicU16::new(0),
            nominated: AtomicBool::new(false),
        }
    }
}

impl fmt::Debug for CandidatePair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "prio {} (local, prio {}) {} <-> {} (remote, prio {})",
            self.priority(),
            self.local.priority(),
            self.local,
            self.remote,
            self.remote.priority()
        )
    }
}

impl fmt::Display for CandidatePair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "prio {} (local, prio {}) {} <-> {} (remote, prio {})",
            self.priority(),
            self.local.priority(),
            self.local,
            self.remote,
            self.remote.priority()
        )
    }
}

impl PartialEq for CandidatePair {
    fn eq(&self, other: &CandidatePair) -> bool {
        self.local.equal(&*other.local) && self.remote.equal(&*other.remote)
    }
}

impl CandidatePair {
    pub fn new(
        local: Arc<dyn Candidate + Send + Sync>,
        remote: Arc<dyn Candidate + Send + Sync>,
        controlling: bool,
    ) -> Self {
        CandidatePair {
            ice_role_controlling: AtomicBool::new(controlling),
            remote,
            local,
            state: AtomicU8::new(CandidatePairState::Waiting as u8),
            binding_request_count: AtomicU16::new(0),
            nominated: AtomicBool::new(false),
        }
    }

    // RFC 5245 - 5.7.2.  Computing Pair Priority and Ordering Pairs
    // Let G be the priority for the candidate provided by the controlling
    // agent.  Let D be the priority for the candidate provided by the
    // controlled agent.
    // pair priority = 2^32*MIN(G,D) + 2*MAX(G,D) + (G>D?1:0)
    pub fn priority(&self) -> u64 {
        let (g, d) = if self.ice_role_controlling.load(Ordering::SeqCst) {
            (self.local.priority(), self.remote.priority())
        } else {
            (self.remote.priority(), self.local.priority())
        };

        // 1<<32 overflows uint32; and if both g && d are
        // maxUint32, this result would overflow uint64
        ((1 << 32u64) - 1) * std::cmp::min(g, d) as u64
            + 2 * std::cmp::max(g, d) as u64
            + if g > d { 1 } else { 0 }
    }

    pub async fn write(&self, b: &[u8]) -> Result<usize, Error> {
        self.local.write_to(b, &*self.remote).await
    }
}

// unmarshal_remote_candidate creates a Remote Candidate from its string representation
pub async fn unmarshal_remote_candidate(
    agent_internal: Arc<Mutex<AgentInternal>>,
    raw: String,
) -> Result<impl Candidate, Error> {
    let split: Vec<&str> = raw.split_whitespace().collect();
    if split.len() < 8 {
        return Err(Error::new(format!(
            "{} ({})",
            *ERR_ATTRIBUTE_TOO_SHORT_ICE_CANDIDATE,
            split.len()
        )));
    }

    // Foundation
    let foundation = split[0].to_owned();

    // Component
    let component: u16 = split[1].parse()?;

    // Network
    let network = split[2].to_owned();

    // Priority
    let priority: u32 = split[3].parse()?;

    // Address
    let address = split[4].to_owned();

    // Port
    let port: u16 = split[5].parse()?;

    let typ = split[7];

    let mut rel_addr = String::new();
    let mut rel_port = 0;
    let mut tcp_type = TcpType::Unspecified;

    if split.len() > 8 {
        let split2 = &split[8..];

        if split2[0] == "raddr" {
            if split2.len() < 4 {
                return Err(Error::new(format!(
                    "{}: incorrect length",
                    *ERR_PARSE_RELATED_ADDR
                )));
            }

            // RelatedAddress
            rel_addr = split2[1].to_owned();

            // RelatedPort
            rel_port = split2[3].parse()?;
        } else if split2[0] == "tcptype" {
            if split2.len() < 2 {
                return Err(Error::new(format!("{}: incorrect length", *ERR_PARSE_TYPE)));
            }

            tcp_type = TcpType::from(split[1]);
        }
    }

    match typ {
        "host" => {
            let config = CandidateHostConfig {
                base_config: CandidateBaseConfig {
                    network,
                    address,
                    port,
                    component,
                    priority,
                    foundation,
                    ..Default::default()
                },
                tcp_type,
            };
            config.new_candidate_host(Some(agent_internal)).await
        }
        "srflx" => {
            let config = CandidateServerReflexiveConfig {
                base_config: CandidateBaseConfig {
                    network,
                    address,
                    port,
                    component,
                    priority,
                    foundation,
                    ..Default::default()
                },
                rel_addr,
                rel_port,
            };
            config
                .new_candidate_server_reflexive(Some(agent_internal))
                .await
        }
        "prflx" => {
            let config = CandidatePeerReflexiveConfig {
                base_config: CandidateBaseConfig {
                    network,
                    address,
                    port,
                    component,
                    priority,
                    foundation,
                    ..Default::default()
                },
                rel_addr,
                rel_port,
            };

            config
                .new_candidate_peer_reflexive(Some(agent_internal))
                .await
        }
        "relay" => {
            let config = CandidateRelayConfig {
                base_config: CandidateBaseConfig {
                    network,
                    address,
                    port,
                    component,
                    priority,
                    foundation,
                    ..Default::default()
                },
                rel_addr,
                rel_port,
                ..Default::default()
            };
            config.new_candidate_relay(Some(agent_internal)).await
        }
        _ => Err(Error::new(format!(
            "{} ({})",
            *ERR_UNKNOWN_CANDIDATE_TYPE, typ
        ))),
    }
}
