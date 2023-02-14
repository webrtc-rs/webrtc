#[cfg(test)]
mod candidate_pair_test;
#[cfg(test)]
mod candidate_relay_test;
#[cfg(test)]
mod candidate_server_reflexive_test;
#[cfg(test)]
mod candidate_test;

pub mod candidate_base;
pub mod candidate_host;
pub mod candidate_peer_reflexive;
pub mod candidate_relay;
pub mod candidate_server_reflexive;

use crate::error::Result;
use crate::network_type::*;
use crate::tcp_type::*;
use candidate_base::*;

use async_trait::async_trait;
use serde::Serialize;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{broadcast, Mutex};

pub(crate) const RECEIVE_MTU: usize = 8192;
pub(crate) const DEFAULT_LOCAL_PREFERENCE: u16 = 65535;

/// Indicates that the candidate is used for RTP.
pub(crate) const COMPONENT_RTP: u16 = 1;
/// Indicates that the candidate is used for RTCP.
pub(crate) const COMPONENT_RTCP: u16 = 0;

/// Candidate represents an ICE candidate
#[async_trait]
pub trait Candidate: fmt::Display {
    /// An arbitrary string used in the freezing algorithm to
    /// group similar candidates.  It is the same for two candidates that
    /// have the same type, base IP address, protocol (UDP, TCP, etc.),
    /// and STUN or TURN server.
    fn foundation(&self) -> String;

    /// A unique identifier for just this candidate
    /// Unlike the foundation this is different for each candidate.
    fn id(&self) -> String;

    /// A component is a piece of a data stream.
    /// An example is one for RTP, and one for RTCP
    fn component(&self) -> u16;
    fn set_component(&self, c: u16);

    /// The last time this candidate received traffic
    fn last_received(&self) -> SystemTime;

    /// The last time this candidate sent traffic
    fn last_sent(&self) -> SystemTime;

    fn network_type(&self) -> NetworkType;
    fn address(&self) -> String;
    fn port(&self) -> u16;

    fn priority(&self) -> u32;

    /// A transport address related to candidate,
    /// which is useful for diagnostics and other purposes.
    fn related_address(&self) -> Option<CandidateRelatedAddress>;

    fn candidate_type(&self) -> CandidateType;
    fn tcp_type(&self) -> TcpType;

    fn marshal(&self) -> String;

    fn addr(&self) -> SocketAddr;

    async fn close(&self) -> Result<()>;
    fn seen(&self, outbound: bool);

    async fn write_to(&self, raw: &[u8], dst: &(dyn Candidate + Send + Sync)) -> Result<usize>;
    fn equal(&self, other: &dyn Candidate) -> bool;
    fn set_ip(&self, ip: &IpAddr) -> Result<()>;
    fn get_conn(&self) -> Option<&Arc<dyn util::Conn + Send + Sync>>;
    fn get_closed_ch(&self) -> Arc<Mutex<Option<broadcast::Sender<()>>>>;
}

/// Represents the type of candidate `CandidateType` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CandidateType {
    #[serde(rename = "unspecified")]
    Unspecified,
    #[serde(rename = "host")]
    Host,
    #[serde(rename = "srflx")]
    ServerReflexive,
    #[serde(rename = "prflx")]
    PeerReflexive,
    #[serde(rename = "relay")]
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
        write!(f, "{s}")
    }
}

impl Default for CandidateType {
    fn default() -> Self {
        Self::Unspecified
    }
}

impl CandidateType {
    /// Returns the preference weight of a `CandidateType`.
    ///
    /// 4.1.2.2.  Guidelines for Choosing Type and Local Preferences
    /// The RECOMMENDED values are 126 for host candidates, 100
    /// for server reflexive candidates, 110 for peer reflexive candidates,
    /// and 0 for relayed candidates.
    #[must_use]
    pub const fn preference(self) -> u16 {
        match self {
            Self::Host => 126,
            Self::PeerReflexive => 110,
            Self::ServerReflexive => 100,
            Self::Relay | CandidateType::Unspecified => 0,
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

/// Convey transport addresses related to the candidate, useful for diagnostics and other purposes.
#[derive(PartialEq, Eq, Debug, Clone)]
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

/// Represent the ICE candidate pair state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CandidatePairState {
    #[serde(rename = "unspecified")]
    Unspecified = 0,

    /// Means a check has not been performed for this pair.
    #[serde(rename = "waiting")]
    Waiting = 1,

    /// Means a check has been sent for this pair, but the transaction is in progress.
    #[serde(rename = "in-progress")]
    InProgress = 2,

    /// Means a check for this pair was already done and failed, either never producing any response
    /// or producing an unrecoverable failure response.
    #[serde(rename = "failed")]
    Failed = 3,

    /// Means a check for this pair was already done and produced a successful result.
    #[serde(rename = "succeeded")]
    Succeeded = 4,
}

impl From<u8> for CandidatePairState {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::Waiting,
            2 => Self::InProgress,
            3 => Self::Failed,
            4 => Self::Succeeded,
            _ => Self::Unspecified,
        }
    }
}

impl Default for CandidatePairState {
    fn default() -> Self {
        Self::Unspecified
    }
}

impl fmt::Display for CandidatePairState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Self::Waiting => "waiting",
            Self::InProgress => "in-progress",
            Self::Failed => "failed",
            Self::Succeeded => "succeeded",
            Self::Unspecified => "unspecified",
        };

        write!(f, "{s}")
    }
}

/// Represents a combination of a local and remote candidate.
pub struct CandidatePair {
    pub(crate) ice_role_controlling: AtomicBool,
    pub remote: Arc<dyn Candidate + Send + Sync>,
    pub local: Arc<dyn Candidate + Send + Sync>,
    pub(crate) binding_request_count: AtomicU16,
    pub(crate) state: AtomicU8, // convert it to CandidatePairState,
    pub(crate) nominated: AtomicBool,
}

impl Default for CandidatePair {
    fn default() -> Self {
        Self {
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
    fn eq(&self, other: &Self) -> bool {
        self.local.equal(&*other.local) && self.remote.equal(&*other.remote)
    }
}

impl CandidatePair {
    #[must_use]
    pub fn new(
        local: Arc<dyn Candidate + Send + Sync>,
        remote: Arc<dyn Candidate + Send + Sync>,
        controlling: bool,
    ) -> Self {
        Self {
            ice_role_controlling: AtomicBool::new(controlling),
            remote,
            local,
            state: AtomicU8::new(CandidatePairState::Waiting as u8),
            binding_request_count: AtomicU16::new(0),
            nominated: AtomicBool::new(false),
        }
    }

    /// RFC 5245 - 5.7.2.  Computing Pair Priority and Ordering Pairs
    /// Let G be the priority for the candidate provided by the controlling
    /// agent.  Let D be the priority for the candidate provided by the
    /// controlled agent.
    /// pair priority = 2^32*MIN(G,D) + 2*MAX(G,D) + (G>D?1:0)
    pub fn priority(&self) -> u64 {
        let (g, d) = if self.ice_role_controlling.load(Ordering::SeqCst) {
            (self.local.priority(), self.remote.priority())
        } else {
            (self.remote.priority(), self.local.priority())
        };

        // 1<<32 overflows uint32; and if both g && d are
        // maxUint32, this result would overflow uint64
        ((1 << 32_u64) - 1) * u64::from(std::cmp::min(g, d))
            + 2 * u64::from(std::cmp::max(g, d))
            + u64::from(g > d)
    }

    pub async fn write(&self, b: &[u8]) -> Result<usize> {
        self.local.write_to(b, &*self.remote).await
    }
}
