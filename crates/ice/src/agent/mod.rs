#[cfg(test)]
mod agent_test;

pub mod agent_config;
pub mod agent_stats;

use crate::candidate::candidate_pair::*;
use crate::candidate::candidate_type::*;
use crate::candidate::*;
use crate::errors::*;
use crate::external_ip_mapper::*;
use crate::mdns::*;
use crate::network_type::*;
use crate::state::*;
use crate::url::*;

use mdns::conn::*;
use stun::agent::TransactionId;
use util::{Buffer, Error};

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::agent::agent_config::{AgentConfig, MAX_BUFFER_SIZE};
use crate::selector::PairCandidateSelector;
//use std::sync::Arc;
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
#[derive(Default)]
pub struct Agent {
    chan_task: Option<mpsc::Receiver<Task>>,
    //TODO: afterRunFn []func(ctx context.Context)
    //TODO: muAfterRun sync.Mutex

    //TODO: onConnectionStateChangeHdlr       atomic.Value // func(ConnectionState)
    //TODO: onSelectedCandidatePairChangeHdlr atomic.Value // func(Candidate, Candidate)
    //TODO: onCandidateHdlr                   atomic.Value // func(Candidate)

    // State owned by the taskLoop
    on_connected: Option<oneshot::Receiver<()>>,

    // force candidate to be contacted immediately (instead of waiting for task ticker)
    force_candidate_contact: Option<mpsc::Receiver<bool>>,
    tie_breaker: u64,
    lite: bool,

    connection_state: ConnectionState,
    gathering_state: GatheringState,

    mdns_mode: MulticastDNSMode,
    mdns_name: String,
    mdns_conn: Option<DNSConn>,

    started_ch: Option<mpsc::Receiver<()>>,
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
    selector: Option<Box<dyn PairCandidateSelector>>,

    selected_pair: CandidatePair, //TODO: atomic.Value
    urls: Vec<URL>,
    network_types: Vec<NetworkType>,

    buffer: Option<Buffer>,

    // LRU of outbound Binding request Transaction IDs
    pending_binding_requests: Vec<BindingRequest>,

    // 1:1 D-NAT IP address mapping
    ext_ip_mapper: ExternalIPMapper,

    // State for closing
    done: Option<mpsc::Receiver<()>>,
    //TODO: err  atomicError

    //TODO: gatherCandidateCancel func()
    chan_candidate: Option<mpsc::Receiver<Box<dyn Candidate + Send + Sync>>>,
    chan_candidate_pair: Option<mpsc::Receiver<CandidatePair>>,
    chan_state: Option<mpsc::Receiver<ConnectionState>>,

    //TODO: net    *vnet.Net
    //TODO: tcpMux TCPMux
    interface_filter: Option<Box<dyn Fn(String) -> bool>>,

    insecure_skip_verify: bool,
    //TODO: proxyDialer proxy.Dialer
}

impl Agent {
    // new creates a new Agent
    pub async fn new(mut config: AgentConfig) -> Result<Agent, Error> {
        if config.port_max < config.port_min {
            return Err(ERR_PORT.to_owned());
        }

        let mut mdns_name = config.multicast_dnshost_name.clone();
        if mdns_name.is_empty() {
            mdns_name = generate_multicast_dns_name();
        }

        if !mdns_name.ends_with(".local") || mdns_name.split('.').count() != 2 {
            return Err(ERR_INVALID_MULTICAST_DNSHOST_NAME.to_owned());
        }

        let mut mdns_mode = config.multicast_dns_mode;
        if mdns_mode == MulticastDNSMode::Disabled {
            mdns_mode = MulticastDNSMode::QueryOnly;
        }

        let mdns_conn = match create_multicast_dns(mdns_mode, &mdns_name) {
            Ok(c) => c,
            Err(err) => {
                // Opportunistic mDNS: If we can't open the connection, that's ok: we
                // can continue without it.
                log::warn!("Failed to initialize mDNS {}: {}", mdns_name, err);
                None
            }
        };

        //startedCtx, startedFn := context.WithCancel(context.Background())

        let mut a = Agent {
            /*chanTask:          make(chan task),
            chanState:         make(chan ConnectionState),
            chanCandidate:     make(chan Candidate),
            chanCandidatePair: make(chan *candidatePair),
            */
            tie_breaker: rand::random::<u64>(),
            lite: config.lite,
            gathering_state: GatheringState::New,
            connection_state: ConnectionState::New,
            local_candidates: HashMap::new(),
            remote_candidates: HashMap::new(),
            urls: config.urls.clone(),
            network_types: config.network_types.clone(),
            //TODO: onConnected:       make(chan struct{}),
            // Make sure the buffer doesn't grow indefinitely.
            // NOTE: We actually won't get anywhere close to this limit.
            // SRTP will constantly read from the endpoint and drop packets if it's full.
            buffer: Some(Buffer::new(0, MAX_BUFFER_SIZE)),
            //TODO:done:              make(chan struct{}),
            //TODO:startedCh:         startedCtx.Done(),
            //TODO:startedFn:         startedFn,
            port_min: config.port_min,
            port_max: config.port_max,

            mdns_mode,
            mdns_name,
            mdns_conn,

            //TODO: gatherCandidateCancel: func() {},
            //TODO: forceCandidateContact: make(chan bool, 1),
            interface_filter: config.interface_filter.take(),
            insecure_skip_verify: config.insecure_skip_verify,

            ..Default::default()
        };

        /*a.tcpMux = config.TCPMux
        if a.tcpMux == nil {
            a.tcpMux = newInvalidTCPMux()
        }

        if a.net == nil {
            a.net = vnet.NewNet(nil)
        } else if a.net.IsVirtual() {
            a.log.Warn("vnet is enabled")
            if a.mDNSMode != MulticastDNSModeDisabled {
                a.log.Warn("vnet does not support mDNS yet")
            }
        }*/

        config.init_with_defaults(&mut a);

        if a.lite && (a.candidate_types.len() != 1 || a.candidate_types[0] != CandidateType::Host) {
            if let Some(c) = &a.mdns_conn {
                if let Err(err) = c.close().await {
                    log::warn!("Failed to close mDNS: {}", err)
                }
            }
            return Err(ERR_LITE_USING_NON_HOST_CANDIDATES.to_owned());
        }

        if !config.urls.is_empty()
            && !contains_candidate_type(CandidateType::ServerReflexive, &a.candidate_types)
            && !contains_candidate_type(CandidateType::Relay, &a.candidate_types)
        {
            if let Some(c) = &a.mdns_conn {
                if let Err(err) = c.close().await {
                    log::warn!("Failed to close mDNS: {}", err)
                }
            }
            return Err(ERR_USELESS_URLS_PROVIDED.to_owned());
        }

        if let Err(err) = config.init_ext_ip_mapping(&mut a) {
            if let Some(c) = &a.mdns_conn {
                if let Err(err) = c.close().await {
                    log::warn!("Failed to close mDNS: {}", err)
                }
            }
            return Err(err);
        }

        /* TODO:
        go a.taskLoop()
        a.startOnConnectionStateChangeRoutine()

        // Restart is also used to initialize the agent for the first time
        if err := a.Restart(config.LocalUfrag, config.LocalPwd); err != nil {
            if let Some(c) = &a.mdns_conn {
                if let Err(err) = c.close().await {
                    log::warn!("Failed to close mDNS: {}", err)
                }
            }
            _ = a.Close()
            return nil, err
        }
        */
        Ok(a)
    }
}
