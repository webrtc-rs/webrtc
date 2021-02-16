#[cfg(test)]
mod agent_test;

pub mod agent_config;
pub mod agent_gather;
pub(crate) mod agent_internal;
pub mod agent_selector;
pub mod agent_stats;
pub mod agent_transport;

use crate::candidate::*;
use crate::errors::*;
use crate::external_ip_mapper::*;
use crate::mdns::*;
use crate::network_type::*;
use crate::state::*;
use crate::url::*;
use agent_internal::*;

use mdns::conn::*;
use stun::{agent::*, attributes::*, fingerprint::*, integrity::*, message::*, xoraddr::*};
use util::{Buffer, Error};

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::agent::agent_config::{AgentConfig, MAX_BINDING_REQUEST_TIMEOUT, MAX_BUFFER_SIZE};
use crate::rand::*;

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::{Duration, Instant};

pub(crate) struct BindingRequest {
    pub(crate) timestamp: Instant,
    pub(crate) transaction_id: TransactionId,
    pub(crate) destination: SocketAddr,
    pub(crate) is_use_candidate: bool,
}

pub type OnConnectionStateChangeHdlrFn = Box<dyn Fn(ConnectionState) + Send + Sync>;
pub type OnSelectedCandidatePairChangeHdlrFn =
    Box<dyn Fn(&(dyn Candidate + Send + Sync), &(dyn Candidate + Send + Sync)) + Send + Sync>;
pub type OnCandidateHdlrFn = Box<dyn Fn(Arc<dyn Candidate + Send + Sync>) + Send + Sync>;
pub type GatherCandidateCancelFn = Box<dyn Fn() + Send + Sync>;

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
            is_controlling: config.is_controlling,
            start_time: Instant::now(),
            nominated_pair: None,

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

            started_ch_tx: None,

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

            // LRU of outbound Binding request Transaction IDs
            pending_binding_requests: vec![],

            // 1:1 D-NAT IP address mapping
            ext_ip_mapper: ExternalIPMapper::default(),
            bytes_received: Arc::new(AtomicUsize::new(0)),
            bytes_sent: Arc::new(AtomicUsize::new(0)),
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
        mut chan_candidate_rx: mpsc::Receiver<Arc<dyn Candidate + Send + Sync>>,
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
        ai.delete_all_candidates().await;
        ai.start();

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
