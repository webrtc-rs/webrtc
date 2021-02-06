#[cfg(test)]
mod agent_test;

pub mod agent_config;
pub mod agent_stats;

use crate::candidate::candidate_pair::*;
use crate::candidate::candidate_type::*;
use crate::candidate::*;
use crate::external_ip_mapper::*;
use crate::mdns::*;
use crate::network_type::*;
use crate::state::*;
use crate::url::*;

use mdns::conn::*;
use stun::agent::TransactionId;
use util::Buffer;

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::selector::PairCandidateSelector;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::{Duration, Instant};

pub(crate) struct BindingRequest {
    timestamp: Instant,
    transaction_id: TransactionId,
    destination: SocketAddr,
    is_use_candidate: bool,
}

struct Task {
    //TODO: func(context.Context, *Agent)
    done: mpsc::Sender<()>,
}

// Agent represents the ICE agent
pub struct Agent {
    chan_task: mpsc::Receiver<Task>,
    //TODO: afterRunFn []func(ctx context.Context)
    //TODO: muAfterRun sync.Mutex

    //TODO: onConnectionStateChangeHdlr       atomic.Value // func(ConnectionState)
    //TODO: onSelectedCandidatePairChangeHdlr atomic.Value // func(Candidate, Candidate)
    //TODO: onCandidateHdlr                   atomic.Value // func(Candidate)

    // State owned by the taskLoop
    on_connected: oneshot::Receiver<()>,

    // force candidate to be contacted immediately (instead of waiting for task ticker)
    force_candidate_contact: mpsc::Receiver<bool>,
    tie_breaker: u64,
    lite: bool,

    connection_state: ConnectionState,
    gathering_state: GatheringState,

    mdns_mode: MulticastDNSMode,
    mdns_name: String,
    mdns_conn: DNSConn,

    started_ch: mpsc::Receiver<()>,
    //TODO: startedFn     func()
    is_controlling: bool,

    max_binding_requests: u16,

    pub(crate) host_acceptance_min_wait: Duration,
    pub(crate) srflx_acceptance_min_wait: Duration,
    pub(crate) prflx_acceptance_min_wait: Duration,
    pub(crate) relay_acceptance_min_wait: Duration,

    port_min: u16,
    port_max: u16,

    candidate_types: Vec<CandidateType>,

    // How long connectivity checks can fail before the ICE Agent
    // goes to disconnected
    disconnected_timeout: Duration,

    // How long connectivity checks can fail before the ICE Agent
    // goes to failed
    failed_timeout: Duration,

    // How often should we send keepalive packets?
    // 0 means never
    keepalive_interval: Duration,

    // How often should we run our internal taskLoop to check for state changes when connecting
    check_interval: Duration,

    local_ufrag: String,
    local_pwd: String,
    local_candidates: HashMap<NetworkType, Vec<Box<dyn Candidate>>>,

    remote_ufrag: String,
    remote_pwd: String,
    remote_candidates: HashMap<NetworkType, Vec<Box<dyn Candidate>>>,

    checklist: Vec<CandidatePair>,
    selector: Box<dyn PairCandidateSelector>,

    selected_pair: CandidatePair, //TODO: atomic.Value
    urls: Vec<URL>,
    network_types: Vec<NetworkType>,

    buffer: Buffer,

    // LRU of outbound Binding request Transaction IDs
    pending_binding_requests: Vec<BindingRequest>,

    // 1:1 D-NAT IP address mapping
    ext_ip_mapper: ExternalIPMapper,

    // State for closing
    done: mpsc::Receiver<()>,
    //TODO: err  atomicError

    //TODO: gatherCandidateCancel func()
    chan_candidate: mpsc::Receiver<Box<dyn Candidate + Send + Sync>>,
    chan_candidate_pair: mpsc::Receiver<CandidatePair>,
    chan_state: mpsc::Receiver<ConnectionState>,

    //TODO: net    *vnet.Net
    //TODO: tcpMux TCPMux
    interface_filter: Option<fn(String) -> bool>,

    insecure_skip_verify: bool,
    //TODO: proxyDialer proxy.Dialer
}
