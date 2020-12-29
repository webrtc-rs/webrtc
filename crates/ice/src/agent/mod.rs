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

use stun::agent::TransactionId;
//use util::Error;

use std::collections::HashMap;
use std::net::SocketAddr;

use tokio::time::{Duration, Instant};

pub(crate) struct BindingRequest {
    timestamp: Instant,
    transaction_id: TransactionId,
    destination: SocketAddr,
    is_use_candidate: bool,
}

// Agent represents the ICE agent
pub struct Agent {
    //TODO: chanTask   chan task
    //TODO: afterRunFn []func(ctx context.Context)
    //TODO: muAfterRun sync.Mutex

    //TODO: onConnectionStateChangeHdlr       atomic.Value // func(ConnectionState)
    //TODO: onSelectedCandidatePairChangeHdlr atomic.Value // func(Candidate, Candidate)
    //TODO: onCandidateHdlr                   atomic.Value // func(Candidate)

    // State owned by the taskLoop
    //TODO: onConnected     chan struct{}
    //TODO: onConnectedOnce sync.Once

    // force candidate to be contacted immediately (instead of waiting for task ticker)
    //TODO: forceCandidateContact chan bool
    tie_breaker: u64,
    lite: bool,

    connection_state: ConnectionState,
    gathering_state: GatheringState,

    mdns_mode: MulticastDNSMode,
    mdns_name: String,
    //TODO: mDNSConn *mdns.Conn

    //TODO: muHaveStarted sync.Mutex
    //TODO: startedCh     <-chan struct{}
    //TODO: startedFn     func()
    is_controlling: bool,

    max_binding_requests: u16,

    host_acceptance_min_wait: Duration,
    srflx_acceptance_min_wait: Duration,
    prflx_acceptance_min_wait: Duration,
    relay_acceptance_min_wait: Duration,

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
    //TODO: selector  pairCandidateSelector

    //TODO: selectedPair atomic.Value // *candidatePair
    urls: Vec<URL>,
    network_types: Vec<NetworkType>,

    //TODO: buffer *packetio.Buffer

    // LRU of outbound Binding request Transaction IDs
    pending_binding_requests: Vec<BindingRequest>,

    // 1:1 D-NAT IP address mapping
    ext_ip_mapper: ExternalIPMapper,

    // State for closing
    //TODO: done chan struct{}
    //TODO: err  atomicError

    //TODO: gatherCandidateCancel func()

    //TODO: chanCandidate     chan Candidate
    //TODO: chanCandidatePair chan *candidatePair
    //TODO: chanState         chan ConnectionState

    //TODO: loggerFactory logging.LoggerFactory
    //TODO: log           logging.LeveledLogger

    //TODO: net    *vnet.Net
    //TODO: tcpMux TCPMux
    interface_filter: Option<fn(String) -> bool>,

    insecure_skip_verify: bool,
    //TODO: proxyDialer proxy.Dialer
}
