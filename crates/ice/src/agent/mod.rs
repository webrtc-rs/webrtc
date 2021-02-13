#[cfg(test)]
mod agent_test;

pub mod agent_config;
pub mod agent_gather;
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
use stun::{agent::*, attributes::*, fingerprint::*, integrity::*, message::*, xoraddr::*};
use util::{Buffer, Error};

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::agent::agent_config::{AgentConfig, MAX_BINDING_REQUEST_TIMEOUT, MAX_BUFFER_SIZE};
use crate::rand::*;
use crate::selector::PairCandidateSelector;

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{Duration, Instant};

pub(crate) struct BindingRequest {
    timestamp: Instant,
    transaction_id: TransactionId,
    pub(crate) destination: SocketAddr,
    pub(crate) is_use_candidate: bool,
}

pub type OnConnectionStateChangeHdlrFn = Box<dyn Fn(ConnectionState) + Send + Sync>;
pub type OnSelectedCandidatePairChangeHdlrFn =
    Box<dyn Fn(&(dyn Candidate + Send + Sync), &(dyn Candidate + Send + Sync)) + Send + Sync>;
pub type OnCandidateHdlrFn = Box<dyn Fn(Box<dyn Candidate + Send + Sync>) + Send + Sync>;
pub type GatherCandidateCancelFn = Box<dyn Fn() + Send + Sync>;

pub struct AgentInternal {
    // State owned by the taskLoop
    on_connected_tx: Option<mpsc::Sender<()>>,

    // State for closing
    done: Option<mpsc::Sender<()>>,
    chan_candidate: Option<mpsc::Sender<Box<dyn Candidate + Send + Sync>>>,
    chan_candidate_pair: Option<mpsc::Sender<()>>,
    chan_state: Option<mpsc::Sender<ConnectionState>>,

    on_connection_state_change_hdlr: Option<OnConnectionStateChangeHdlrFn>,
    on_selected_candidate_pair_change_hdlr: Option<OnSelectedCandidatePairChangeHdlrFn>,
    on_candidate_hdlr: Option<OnCandidateHdlrFn>,
    selected_pair: Option<CandidatePair>,

    on_connected_rx: mpsc::Receiver<()>,

    // force candidate to be contacted immediately (instead of waiting for task ticker)
    force_candidate_contact: Option<mpsc::Receiver<bool>>,
    pub(crate) tie_breaker: u64,
    lite: bool,

    connection_state: ConnectionState,
    gathering_state: GatheringState,

    mdns_mode: MulticastDNSMode,
    mdns_name: String,
    mdns_conn: Option<DNSConn>,

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

    pub(crate) local_ufrag: String,
    local_pwd: String,
    local_candidates: HashMap<NetworkType, Vec<Box<dyn Candidate + Send + Sync>>>,

    pub(crate) remote_ufrag: String,
    pub(crate) remote_pwd: String,
    remote_candidates: HashMap<NetworkType, Vec<Box<dyn Candidate + Send + Sync>>>,

    checklist: Vec<CandidatePair>,
    selector: Option<Box<dyn PairCandidateSelector + Send + Sync>>,

    urls: Vec<URL>,
    network_types: Vec<NetworkType>,

    buffer: Option<Buffer>,

    // LRU of outbound Binding request Transaction IDs
    pending_binding_requests: Vec<BindingRequest>,

    // 1:1 D-NAT IP address mapping
    ext_ip_mapper: ExternalIPMapper,

    //TODO: err  atomicError
    gather_candidate_cancel: Option<GatherCandidateCancelFn>,

    //TODO: net    *vnet.Net
    //TODO: tcpMux TCPMux
    interface_filter: Option<Box<dyn Fn(String) -> bool>>,

    insecure_skip_verify: bool,
    //TODO: proxyDialer proxy.Dialer
}

//TODO: remove unsafe
unsafe impl Send for AgentInternal {}
unsafe impl Sync for AgentInternal {}

impl AgentInternal {
    fn close(&mut self) -> Result<(), Error> {
        if self.done.is_none() {
            return Err(ERR_CLOSED.to_owned());
        }

        if let Some(gather_candidate_cancel) = &self.gather_candidate_cancel {
            gather_candidate_cancel();
        }

        //TODO: ? a.tcpMux.RemoveConnByUfrag(a.localUfrag)

        self.done.take();

        Ok(())
    }

    pub(crate) fn get_selected_pair(&self) -> Option<&CandidatePair> {
        self.selected_pair.as_ref()
    }

    pub(crate) async fn set_selected_pair(&mut self, p: Option<CandidatePair>) {
        log::trace!("Set selected candidate pair: {:?}", p);

        if let Some(mut p) = p {
            p.nominated = true;
            self.selected_pair = Some(p);

            self.update_connection_state(ConnectionState::Connected)
                .await;

            // Notify when the selected pair changes
            if let Some(chan_candidate_pair) = &self.chan_candidate_pair {
                let _ = chan_candidate_pair.send(()).await;
            }

            // Signal connected
            self.on_connected_tx.take();
        } else {
            self.selected_pair = None;
        }
    }

    async fn update_connection_state(&mut self, new_state: ConnectionState) {
        if self.connection_state != new_state {
            // Connection has gone to failed, release all gathered candidates
            if new_state == ConnectionState::Failed {
                self.delete_all_candidates();
            }

            log::info!("Setting new connection state: {}", new_state);
            self.connection_state = new_state;

            // Call handler after finishing current task since we may be holding the agent lock
            // and the handler may also require it
            if let Some(chan_state) = &self.chan_state {
                let _ = chan_state.send(new_state).await;
            }
        }
    }

    // Remove all candidates. This closes any listening sockets
    // and removes both the local and remote candidate lists.
    //
    // This is used for restarts, failures and on close
    fn delete_all_candidates(&mut self) {
        for cs in &mut self.local_candidates.values_mut() {
            for c in cs {
                if let Err(err) = c.close() {
                    log::warn!("Failed to close candidate {}: {}", c, err);
                }
            }
        }
        self.local_candidates.clear();

        for cs in self.remote_candidates.values_mut() {
            for c in cs {
                if let Err(err) = c.close() {
                    log::warn!("Failed to close candidate {}: {}", c, err);
                }
            }
        }
        self.remote_candidates.clear();
    }

    pub(crate) fn get_best_available_candidate_pair(&self) -> Option<&CandidatePair> {
        let mut best: Option<&CandidatePair> = None;

        for p in &self.checklist {
            if p.state == CandidatePairState::Failed {
                continue;
            }

            if let Some(b) = &mut best {
                if b.priority() < p.priority() {
                    *b = p;
                }
            } else {
                best = Some(p);
            }
        }

        best
    }

    pub(crate) fn get_best_available_candidate_pair_mut(&mut self) -> Option<&mut CandidatePair> {
        let mut best: Option<&mut CandidatePair> = None;

        for p in &mut self.checklist {
            if p.state == CandidatePairState::Failed {
                continue;
            }

            if let Some(b) = &mut best {
                if b.priority() < p.priority() {
                    *b = p;
                }
            } else {
                best = Some(p);
            }
        }

        best
    }

    pub(crate) fn add_pair(
        &mut self,
        local: Box<dyn Candidate + Send + Sync>,
        remote: Box<dyn Candidate + Send + Sync>,
    ) /*-> Option<&CandidatePair>*/
    {
        let p = CandidatePair::new(local, remote, self.is_controlling);
        self.checklist.push(p);
        //return p;
    }

    pub(crate) fn find_pair(
        &self,
        local: &(dyn Candidate + Send + Sync),
        remote: &(dyn Candidate + Send + Sync),
    ) -> Option<&CandidatePair> {
        for p in &self.checklist {
            if p.local.equal(local) && p.remote.equal(remote) {
                return Some(p);
            }
        }
        None
    }

    pub(crate) fn get_pair_mut(
        &mut self,
        local: &(dyn Candidate + Send + Sync),
        remote: &(dyn Candidate + Send + Sync),
    ) -> Option<&mut CandidatePair> {
        for p in &mut self.checklist {
            if p.local.equal(local) && p.remote.equal(remote) {
                return Some(p);
            }
        }
        None
    }

    // validate_selected_pair checks if the selected pair is (still) valid
    // Note: the caller should hold the agent lock.
    pub(crate) async fn validate_selected_pair(&mut self) -> bool {
        if let Some(selected_pair) = &self.selected_pair {
            let disconnected_time =
                Instant::now().duration_since(selected_pair.remote.last_received());

            // Only allow transitions to failed if a.failedTimeout is non-zero
            let mut total_time_to_failure = self.failed_timeout;
            if total_time_to_failure != Duration::from_secs(0) {
                total_time_to_failure += self.disconnected_timeout;
            }

            if total_time_to_failure != Duration::from_secs(0)
                && disconnected_time > total_time_to_failure
            {
                self.update_connection_state(ConnectionState::Failed).await;
            } else if self.disconnected_timeout != Duration::from_secs(0)
                && disconnected_time > self.disconnected_timeout
            {
                self.update_connection_state(ConnectionState::Disconnected)
                    .await;
            } else {
                self.update_connection_state(ConnectionState::Connected)
                    .await;
            }

            true
        } else {
            false
        }
    }

    // checkKeepalive sends STUN Binding Indications to the selected pair
    // if no packet has been sent on that pair in the last keepaliveInterval
    // Note: the caller should hold the agent lock.
    pub(crate) async fn check_keepalive(&mut self) {
        if let Some(selected_pair) = &self.selected_pair {
            if (self.keepalive_interval != Duration::from_secs(0))
                && ((Instant::now().duration_since(selected_pair.local.last_sent())
                    > self.keepalive_interval)
                    || (Instant::now().duration_since(selected_pair.remote.last_received())
                        > self.keepalive_interval))
            {
                // we use binding request instead of indication to support refresh consent schemas
                // see https://tools.ietf.org/html/rfc7675
                if let Some(selector) = &mut self.selector {
                    selector
                        .ping_candidate(&*selected_pair.local, &*selected_pair.remote)
                        .await;
                }
            }
        }
    }

    pub(crate) async fn ping_all_candidates(&mut self) {
        log::trace!("pinging all candidates");

        if self.checklist.is_empty() {
            log::warn!(
                "pingAllCandidates called with no candidate pairs. Connection is not possible yet."
            );
        }

        for p in &mut self.checklist {
            if p.state == CandidatePairState::Waiting {
                p.state = CandidatePairState::InProgress;
            } else if p.state != CandidatePairState::InProgress {
                continue;
            }

            if p.binding_request_count > self.max_binding_requests {
                log::trace!("max requests reached for pair {}, marking it as failed", p);
                p.state = CandidatePairState::Failed;
            } else if let Some(selector) = &mut self.selector {
                selector.ping_candidate(&*(p.local), &*(p.remote)).await;
                p.binding_request_count += 1;
            }
        }
    }

    pub(crate) fn send_binding_request(
        &mut self,
        m: &Message,
        local: &(dyn Candidate + Send + Sync),
        remote: &(dyn Candidate + Send + Sync),
    ) {
        log::trace!("ping STUN from {} to {}", local, remote);

        self.invalidate_pending_binding_requests(Instant::now());
        self.pending_binding_requests.push(BindingRequest {
            timestamp: Instant::now(),
            transaction_id: m.transaction_id,
            destination: remote.addr(),
            is_use_candidate: m.contains(ATTR_USE_CANDIDATE),
        });

        self.send_stun(m, local, remote);
    }

    pub(crate) fn send_binding_success(
        &mut self,
        m: &Message,
        local: &(dyn Candidate + Send + Sync),
        remote: &(dyn Candidate + Send + Sync),
    ) {
        let (ip, port) = (remote.addr().ip(), remote.addr().port());

        let mut out = Message::new();
        if let Err(err) = out.build(&[
            Box::new(m.clone()),
            Box::new(BINDING_SUCCESS),
            Box::new(XORMappedAddress { ip, port }),
            Box::new(MessageIntegrity::new_short_term_integrity(
                self.local_pwd.clone(),
            )),
            Box::new(FINGERPRINT),
        ]) {
            log::warn!(
                "Failed to handle inbound ICE from: {} to: {} error: {}",
                local,
                remote,
                err
            );
        } else {
            self.send_stun(&out, local, remote);
        }
    }

    /* Removes pending binding requests that are over maxBindingRequestTimeout old
       Let HTO be the transaction timeout, which SHOULD be 2*RTT if
       RTT is known or 500 ms otherwise.
       https://tools.ietf.org/html/rfc8445#appendix-B.1
    */
    fn invalidate_pending_binding_requests(&mut self, filter_time: Instant) {
        let initial_size = self.pending_binding_requests.len();

        let mut temp = vec![];
        for binding_request in self.pending_binding_requests.drain(..) {
            if filter_time.duration_since(binding_request.timestamp) < MAX_BINDING_REQUEST_TIMEOUT {
                temp.push(binding_request);
            }
        }

        self.pending_binding_requests = temp;
        let bind_requests_removed = initial_size - self.pending_binding_requests.len();
        if bind_requests_removed > 0 {
            log::trace!(
                "Discarded {} binding requests because they expired",
                bind_requests_removed
            );
        }
    }

    fn send_stun(
        &self,
        _msg: &Message,
        _local: &(dyn Candidate + Send + Sync),
        _remote: &(dyn Candidate + Send + Sync),
    ) {
        /*TODO: if let Err(err) = local.write_to(&msg.raw, remote) {
            log::trace!("failed to send STUN message: {}", err);
        }*/
    }

    // Assert that the passed TransactionID is in our pendingBindingRequests and returns the destination
    // If the bindingRequest was valid remove it from our pending cache
    pub(crate) fn handle_inbound_binding_success(
        &mut self,
        id: TransactionId,
    ) -> Option<BindingRequest> {
        self.invalidate_pending_binding_requests(Instant::now());
        for i in 0..self.pending_binding_requests.len() {
            if self.pending_binding_requests[i].transaction_id == id {
                let valid_binding_request = self.pending_binding_requests.remove(i);
                return Some(valid_binding_request);
            }
        }
        None
    }
}

// Agent represents the ICE agent
pub struct Agent {
    agent_internal: Arc<Mutex<AgentInternal>>,
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

        let (chan_state_tx, chan_state_rx) = mpsc::channel(1);
        let (chan_candidate_tx, chan_candidate_rx) = mpsc::channel(1);
        let (chan_candidate_pair_tx, chan_candidate_pair_rx) = mpsc::channel(1);
        let (on_connected_tx, on_connected_rx) = mpsc::channel(1);

        let mut ai = AgentInternal {
            on_connected_tx: Some(on_connected_tx),
            on_connected_rx,
            chan_state: Some(chan_state_tx),
            chan_candidate: Some(chan_candidate_tx),
            chan_candidate_pair: Some(chan_candidate_pair_tx),
            // State for closing
            done: None,

            on_connection_state_change_hdlr: None,
            on_selected_candidate_pair_change_hdlr: None,
            on_candidate_hdlr: None,
            selected_pair: None,

            tie_breaker: rand::random::<u64>(),
            lite: config.lite,
            gathering_state: GatheringState::New,
            connection_state: ConnectionState::New,
            local_candidates: HashMap::new(),
            remote_candidates: HashMap::new(),
            urls: config.urls.clone(),
            network_types: config.network_types.clone(),

            // Make sure the buffer doesn't grow indefinitely.
            // NOTE: We actually won't get anywhere close to this limit.
            // SRTP will constantly read from the endpoint and drop packets if it's full.
            buffer: Some(Buffer::new(0, MAX_BUFFER_SIZE)),

            port_min: config.port_min,
            port_max: config.port_max,

            mdns_mode,
            mdns_name,
            mdns_conn,

            gather_candidate_cancel: None,

            //TODO: forceCandidateContact: make(chan bool, 1),
            interface_filter: config.interface_filter.take(),
            insecure_skip_verify: config.insecure_skip_verify,

            force_candidate_contact: None,
            is_controlling: false,
            max_binding_requests: 0,

            host_acceptance_min_wait: Duration::from_secs(0),
            srflx_acceptance_min_wait: Duration::from_secs(0),
            prflx_acceptance_min_wait: Duration::from_secs(0),
            relay_acceptance_min_wait: Duration::from_secs(0),
            candidate_types: vec![],

            // How long connectivity checks can fail before the ICE Agent
            // goes to disconnected
            disconnected_timeout: Duration::from_secs(0),

            // How long connectivity checks can fail before the ICE Agent
            // goes to failed
            failed_timeout: Duration::from_secs(0),

            // How often should we send keepalive packets?
            // 0 means never
            keepalive_interval: Duration::from_secs(0),

            // How often should we run our internal taskLoop to check for state changes when connecting
            check_interval: Duration::from_secs(0),

            local_ufrag: String::new(),
            local_pwd: String::new(),

            remote_ufrag: String::new(),
            remote_pwd: String::new(),

            checklist: vec![],
            selector: None,

            // LRU of outbound Binding request Transaction IDs
            pending_binding_requests: vec![],

            // 1:1 D-NAT IP address mapping
            ext_ip_mapper: ExternalIPMapper::default(),
        };

        config.init_with_defaults(&mut ai);

        if ai.lite
            && (ai.candidate_types.len() != 1 || ai.candidate_types[0] != CandidateType::Host)
        {
            if let Some(c) = &ai.mdns_conn {
                if let Err(err) = c.close().await {
                    log::warn!("Failed to close mDNS: {}", err)
                }
            }
            return Err(ERR_LITE_USING_NON_HOST_CANDIDATES.to_owned());
        }

        if !config.urls.is_empty()
            && !contains_candidate_type(CandidateType::ServerReflexive, &ai.candidate_types)
            && !contains_candidate_type(CandidateType::Relay, &ai.candidate_types)
        {
            if let Some(c) = &ai.mdns_conn {
                if let Err(err) = c.close().await {
                    log::warn!("Failed to close mDNS: {}", err)
                }
            }
            return Err(ERR_USELESS_URLS_PROVIDED.to_owned());
        }

        if let Err(err) = config.init_ext_ip_mapping(&mut ai) {
            if let Some(c) = &ai.mdns_conn {
                if let Err(err) = c.close().await {
                    log::warn!("Failed to close mDNS: {}", err)
                }
            }
            return Err(err);
        }

        let mut a = Agent {
            agent_internal: Arc::new(Mutex::new(ai)),
        };

        let agent_internal = Arc::clone(&a.agent_internal);

        let _ = Agent::start_on_connection_state_change_routine(
            agent_internal,
            chan_state_rx,
            chan_candidate_rx,
            chan_candidate_pair_rx,
        )
        .await;

        // Restart is also used to initialize the agent for the first time
        if let Err(err) = a.restart(config.local_ufrag, config.local_pwd).await {
            {
                let ai = a.agent_internal.lock().await;
                if let Some(c) = &ai.mdns_conn {
                    if let Err(err) = c.close().await {
                        log::warn!("Failed to close mDNS: {}", err)
                    }
                }
            }
            let _ = a.close();
            return Err(err);
        }

        Ok(a)
    }

    // Close cleans up the Agent
    pub async fn close(&self) -> Result<(), Error> {
        let mut ai = self.agent_internal.lock().await;
        ai.close()
    }

    async fn start_on_connection_state_change_routine(
        agent_internal: Arc<Mutex<AgentInternal>>,
        mut chan_state_rx: mpsc::Receiver<ConnectionState>,
        mut chan_candidate_rx: mpsc::Receiver<Box<dyn Candidate + Send + Sync>>,
        mut chan_candidate_pair_rx: mpsc::Receiver<()>,
    ) {
        let agent_internal_pair = Arc::clone(&agent_internal);
        tokio::spawn(async move {
            // CandidatePair and ConnectionState are usually changed at once.
            // Blocking one by the other one causes deadlock.
            while chan_candidate_pair_rx.recv().await.is_some() {
                let ai = agent_internal_pair.lock().await;
                if let (Some(on_selected_candidate_pair_change), Some(p)) = (
                    &ai.on_selected_candidate_pair_change_hdlr,
                    &ai.selected_pair,
                ) {
                    on_selected_candidate_pair_change(&*p.local, &*p.remote);
                }
            }
        });

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    opt_state = chan_state_rx.recv() => {
                        let ai = agent_internal.lock().await;
                        if let Some(s) = opt_state {
                            if let Some(on_connection_state_change) = &ai.on_connection_state_change_hdlr{
                                on_connection_state_change(s);
                            }
                        } else {
                            while let Some(c) = chan_candidate_rx.recv().await {
                                if let Some(on_candidate) = &ai.on_candidate_hdlr {
                                    on_candidate(c);
                                }
                            }
                            break;
                        }
                    },
                    opt_cand = chan_candidate_rx.recv() => {
                        let ai = agent_internal.lock().await;
                        if let Some(c) = opt_cand {
                            if let Some(on_candidate) = &ai.on_candidate_hdlr{
                                on_candidate(c);
                            }
                        } else {
                            while let Some(s) = chan_state_rx.recv().await {
                                if let Some(on_connection_state_change) = &ai.on_connection_state_change_hdlr{
                                    on_connection_state_change(s);
                                }
                            }
                            break;
                        }
                    }
                }
            }
        });
    }

    // on_candidate sets a handler that is fired when new candidates gathered. When
    // the gathering process complete the last candidate is nil.
    pub async fn on_candidate(&self, f: OnCandidateHdlrFn) {
        let mut ai = self.agent_internal.lock().await;
        ai.on_candidate_hdlr = Some(f);
    }

    // on_connection_state_change sets a handler that is fired when the connection state changes
    pub async fn on_connection_state_change(&self, f: OnConnectionStateChangeHdlrFn) {
        let mut ai = self.agent_internal.lock().await;
        ai.on_connection_state_change_hdlr = Some(f);
    }

    // on_selected_candidate_pair_change sets a handler that is fired when the final candidate
    // pair is selected
    pub async fn on_selected_candidate_pair_change(&self, f: OnSelectedCandidatePairChangeHdlrFn) {
        let mut ai = self.agent_internal.lock().await;
        ai.on_selected_candidate_pair_change_hdlr = Some(f);
    }

    // Restart restarts the ICE Agent with the provided ufrag/pwd
    // If no ufrag/pwd is provided the Agent will generate one itself
    //
    // Restart must only be called when GatheringState is GatheringStateComplete
    // a user must then call GatherCandidates explicitly to start generating new ones
    pub async fn restart(&mut self, mut ufrag: String, mut pwd: String) -> Result<(), Error> {
        if ufrag.is_empty() {
            ufrag = generate_ufrag();
        }
        if pwd.is_empty() {
            pwd = generate_pwd();
        }

        if ufrag.len() * 8 < 24 {
            return Err(ERR_LOCAL_UFRAG_INSUFFICIENT_BITS.to_owned());
        }
        if pwd.len() * 8 < 128 {
            return Err(ERR_LOCAL_PWD_INSUFFICIENT_BITS.to_owned());
        }

        let mut ai = self.agent_internal.lock().await;

        if ai.gathering_state == GatheringState::Gathering {
            return Err(ERR_RESTART_WHEN_GATHERING.to_owned());
        }

        // Clear all agent needed to take back to fresh state
        ai.local_ufrag = ufrag;
        ai.local_pwd = pwd;
        ai.remote_ufrag = String::new();
        ai.remote_pwd = String::new();

        ai.gathering_state = GatheringState::New;
        ai.checklist = vec![];
        ai.pending_binding_requests = vec![];

        ai.set_selected_pair(None).await;
        ai.delete_all_candidates();
        if let Some(selector) = &mut ai.selector {
            selector.start();
        }

        // Restart is used by NewAgent. Accept/Connect should be used to move to checking
        // for new Agents
        if ai.connection_state != ConnectionState::New {
            ai.update_connection_state(ConnectionState::Checking).await;
        }

        Ok(())
    }

    // get_local_user_credentials returns the local user credentials
    pub async fn get_local_user_credentials(&self) -> (String, String) {
        let ai = self.agent_internal.lock().await;
        (ai.local_ufrag.clone(), ai.local_pwd.clone())
    }

    // get_remote_user_credentials returns the remote user credentials
    pub async fn get_remote_user_credentials(&self) -> (String, String) {
        let ai = self.agent_internal.lock().await;
        (ai.remote_ufrag.clone(), ai.remote_pwd.clone())
    }
}
