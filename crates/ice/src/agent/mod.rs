#[cfg(test)]
mod agent_gather_test;
#[cfg(test)]
mod agent_test;
#[cfg(test)]
mod agent_transport_test;
#[cfg(test)]
pub(crate) mod agent_vnet_test;

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
use agent_config::*;
use agent_internal::*;
use agent_stats::*;

use mdns::conn::*;
use stun::{agent::*, attributes::*, fingerprint::*, integrity::*, message::*, xoraddr::*};
use util::{vnet::net::*, Buffer, Error};

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};

use crate::rand::*;

use crate::agent::agent_gather::GatherCandidatesInternalParams;
use crate::agent::agent_transport::AgentConn;
use crate::candidate::candidate_base::CandidateBaseConfig;
use crate::candidate::candidate_host::CandidateHostConfig;
use crate::candidate::candidate_peer_reflexive::CandidatePeerReflexiveConfig;
use crate::candidate::candidate_relay::CandidateRelayConfig;
use crate::candidate::candidate_server_reflexive::CandidateServerReflexiveConfig;
use crate::tcp_type::TcpType;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct BindingRequest {
    pub(crate) timestamp: Instant,
    pub(crate) transaction_id: TransactionId,
    pub(crate) destination: SocketAddr,
    pub(crate) is_use_candidate: bool,
}

impl Default for BindingRequest {
    fn default() -> Self {
        BindingRequest {
            timestamp: Instant::now(),
            transaction_id: TransactionId::default(),
            destination: SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
            is_use_candidate: false,
        }
    }
}

pub type OnConnectionStateChangeHdlrFn = Box<
    dyn (FnMut(ConnectionState) -> Pin<Box<dyn Future<Output = ()> + Send + Sync>>) + Send + Sync,
>;
pub type OnSelectedCandidatePairChangeHdlrFn = Box<
    dyn (FnMut(
            &(dyn Candidate + Send + Sync),
            &(dyn Candidate + Send + Sync),
        ) -> Pin<Box<dyn Future<Output = ()> + Send + Sync>>)
        + Send
        + Sync,
>;
pub type OnCandidateHdlrFn = Box<dyn FnMut(Option<Arc<dyn Candidate + Send + Sync>>) + Send + Sync>;
pub type GatherCandidateCancelFn = Box<dyn Fn() + Send + Sync>;

// Agent represents the ICE agent
pub struct Agent {
    pub(crate) agent_internal: Arc<Mutex<AgentInternal>>,

    pub(crate) port_min: u16,
    pub(crate) port_max: u16,
    pub(crate) interface_filter: Arc<Option<InterfaceFilterFn>>,
    pub(crate) mdns_mode: MulticastDnsMode,
    pub(crate) mdns_name: String,
    pub(crate) mdns_conn: Option<Arc<DNSConn>>,
    pub(crate) net: Arc<Net>,

    // 1:1 D-NAT IP address mapping
    pub(crate) ext_ip_mapper: Arc<Option<ExternalIpMapper>>,
    pub(crate) gathering_state: Arc<AtomicU8>, //GatheringState,
    pub(crate) candidate_types: Vec<CandidateType>,
    pub(crate) urls: Vec<Url>,
    pub(crate) network_types: Vec<NetworkType>,

    pub(crate) gather_candidate_cancel: Option<GatherCandidateCancelFn>,
}

impl Agent {
    // new creates a new Agent
    pub async fn new(mut config: AgentConfig) -> Result<Agent, Error> {
        if config.port_max < config.port_min {
            return Err(ERR_PORT.to_owned());
        }

        let mut mdns_name = config.multicast_dns_host_name.clone();
        if mdns_name.is_empty() {
            mdns_name = generate_multicast_dns_name();
        }

        if !mdns_name.ends_with(".local") || mdns_name.split('.').count() != 2 {
            return Err(ERR_INVALID_MULTICAST_DNSHOST_NAME.to_owned());
        }

        let mut mdns_mode = config.multicast_dns_mode;
        if mdns_mode == MulticastDnsMode::Unspecified {
            mdns_mode = MulticastDnsMode::QueryOnly;
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
        let (done_tx, done_rx) = mpsc::channel(1);
        let (force_candidate_contact_tx, force_candidate_contact_rx) = mpsc::channel(1);
        let (started_ch_tx, _) = broadcast::channel(1);

        let mut ai = AgentInternal {
            on_connected_tx: Some(on_connected_tx),
            on_connected_rx: Some(on_connected_rx),

            // State for closing
            done_tx: Some(done_tx),
            done_rx: Some(done_rx),

            force_candidate_contact_tx,
            force_candidate_contact_rx: Some(force_candidate_contact_rx),

            chan_state_tx: Some(chan_state_tx),
            chan_candidate_tx: Some(Arc::new(chan_candidate_tx)),
            chan_candidate_pair_tx: Some(chan_candidate_pair_tx),

            on_connection_state_change_hdlr: None,
            on_selected_candidate_pair_change_hdlr: None,
            on_candidate_hdlr: None,

            tie_breaker: rand::random::<u64>(),

            lite: config.lite,
            is_controlling: config.is_controlling,
            start_time: Instant::now(),
            nominated_pair: None,

            connection_state: ConnectionState::New,
            local_candidates: HashMap::new(),
            remote_candidates: HashMap::new(),

            insecure_skip_verify: config.insecure_skip_verify,

            started_ch_tx: Some(started_ch_tx),

            max_binding_requests: 0,

            host_acceptance_min_wait: Duration::from_secs(0),
            srflx_acceptance_min_wait: Duration::from_secs(0),
            prflx_acceptance_min_wait: Duration::from_secs(0),
            relay_acceptance_min_wait: Duration::from_secs(0),

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

            // LRU of outbound Binding request Transaction IDs
            pending_binding_requests: vec![],

            // AgentConn
            agent_conn: Arc::new(AgentConn::new()),
        };

        config.init_with_defaults(&mut ai);

        let candidate_types = if config.candidate_types.is_empty() {
            default_candidate_types()
        } else {
            config.candidate_types.clone()
        };

        if ai.lite && (candidate_types.len() != 1 || candidate_types[0] != CandidateType::Host) {
            Agent::close_multicast_conn(&mdns_conn).await;
            return Err(ERR_LITE_USING_NON_HOST_CANDIDATES.to_owned());
        }

        if !config.urls.is_empty()
            && !contains_candidate_type(CandidateType::ServerReflexive, &candidate_types)
            && !contains_candidate_type(CandidateType::Relay, &candidate_types)
        {
            Agent::close_multicast_conn(&mdns_conn).await;
            return Err(ERR_USELESS_URLS_PROVIDED.to_owned());
        }

        let ext_ip_mapper = match config.init_ext_ip_mapping(mdns_mode, &candidate_types) {
            Ok(ext_ip_mapper) => ext_ip_mapper,
            Err(err) => {
                Agent::close_multicast_conn(&mdns_conn).await;
                return Err(err);
            }
        };

        let net = if let Some(net) = config.net {
            if net.is_virtual() {
                log::warn!("vnet is enabled");
                if mdns_mode != MulticastDnsMode::Disabled {
                    log::warn!("vnet does not support mDNS yet");
                }
            }

            net
        } else {
            Arc::new(Net::new(None))
        };

        let a = Agent {
            port_min: config.port_min,
            port_max: config.port_max,
            agent_internal: Arc::new(Mutex::new(ai)),
            interface_filter: Arc::new(config.interface_filter.take()),
            mdns_mode,
            mdns_name,
            mdns_conn,
            net,
            ext_ip_mapper: Arc::new(ext_ip_mapper),
            gathering_state: Arc::new(AtomicU8::new(0)), //GatheringState::New,
            candidate_types,
            urls: config.urls.clone(),
            network_types: config.network_types.clone(),

            gather_candidate_cancel: None,
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
            Agent::close_multicast_conn(&a.mdns_conn).await;
            let _ = a.close().await;
            return Err(err);
        }

        Ok(a)
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

    // on_candidate sets a handler that is fired when new candidates gathered. When
    // the gathering process complete the last candidate is nil.
    pub async fn on_candidate(&self, f: OnCandidateHdlrFn) {
        let mut ai = self.agent_internal.lock().await;
        ai.on_candidate_hdlr = Some(f);
    }

    async fn start_on_connection_state_change_routine(
        agent_internal: Arc<Mutex<AgentInternal>>,
        mut chan_state_rx: mpsc::Receiver<ConnectionState>,
        mut chan_candidate_rx: mpsc::Receiver<Option<Arc<dyn Candidate + Send + Sync>>>,
        mut chan_candidate_pair_rx: mpsc::Receiver<()>,
    ) {
        let agent_internal_pair = Arc::clone(&agent_internal);
        tokio::spawn(async move {
            // CandidatePair and ConnectionState are usually changed at once.
            // Blocking one by the other one causes deadlock.
            while chan_candidate_pair_rx.recv().await.is_some() {
                let mut ai = agent_internal_pair.lock().await;
                let selected_pair = {
                    let selected_pair = ai.agent_conn.selected_pair.lock().await;
                    selected_pair.clone()
                };

                if let (Some(on_selected_candidate_pair_change), Some(p)) = (
                    &mut ai.on_selected_candidate_pair_change_hdlr,
                    &selected_pair,
                ) {
                    on_selected_candidate_pair_change(&*p.local, &*p.remote).await;
                }
            }
        });

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    opt_state = chan_state_rx.recv() => {
                        let mut ai = agent_internal.lock().await;
                        if let Some(s) = opt_state {
                            if let Some(on_connection_state_change) = &mut ai.on_connection_state_change_hdlr{
                                on_connection_state_change(s).await;
                            }
                        } else {
                            while let Some(c) = chan_candidate_rx.recv().await {
                                if let Some(on_candidate) = &mut ai.on_candidate_hdlr {
                                    on_candidate(c);
                                }
                            }
                            break;
                        }
                    },
                    opt_cand = chan_candidate_rx.recv() => {
                        let mut ai = agent_internal.lock().await;
                        if let Some(c) = opt_cand {
                            if let Some(on_candidate) = &mut ai.on_candidate_hdlr{
                                on_candidate(c);
                            }
                        } else {
                            while let Some(s) = chan_state_rx.recv().await {
                                if let Some(on_connection_state_change) = &mut ai.on_connection_state_change_hdlr{
                                    on_connection_state_change(s).await;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        });
    }

    // add_remote_candidate adds a new remote candidate
    pub async fn add_remote_candidate(
        &self,
        c: &Arc<dyn Candidate + Send + Sync>,
    ) -> Result<(), Error> {
        // cannot check for network yet because it might not be applied
        // when mDNS hostame is used.
        if c.tcp_type() == TcpType::Active {
            // TCP Candidates with tcptype active will probe server passive ones, so
            // no need to do anything with them.
            log::info!("Ignoring remote candidate with tcpType active: {}", c);
            return Ok(());
        }

        // If we have a mDNS Candidate lets fully resolve it before adding it locally
        if c.candidate_type() == CandidateType::Host && c.address().ends_with(".local") {
            if self.mdns_mode == MulticastDnsMode::Disabled {
                log::warn!(
                    "remote mDNS candidate added, but mDNS is disabled: ({})",
                    c.address()
                );
                return Ok(());
            }

            if c.candidate_type() != CandidateType::Host {
                return Err(ERR_ADDRESS_PARSE_FAILED.to_owned());
            }

            let agent_internal = Arc::clone(&self.agent_internal);
            let host_candidate = Arc::clone(c);
            let mdns_conn = self.mdns_conn.clone();
            tokio::spawn(async move {
                if let Some(mdns_conn) = mdns_conn {
                    if let Ok(candidate) =
                        Agent::resolve_and_add_multicast_candidate(mdns_conn, host_candidate).await
                    {
                        let mut ai = agent_internal.lock().await;
                        ai.add_remote_candidate(&candidate).await;
                    }
                }
            });
        } else {
            let agent_internal = Arc::clone(&self.agent_internal);
            let candidate = Arc::clone(c);
            tokio::spawn(async move {
                let mut ai = agent_internal.lock().await;
                ai.add_remote_candidate(&candidate).await;
            });
        }

        Ok(())
    }

    // get_local_candidates returns the local candidates
    pub async fn get_local_candidates(
        &self,
    ) -> Result<Vec<Arc<dyn Candidate + Send + Sync>>, Error> {
        let mut res = vec![];

        {
            let ai = self.agent_internal.lock().await;
            for candidates in ai.local_candidates.values() {
                for candidate in candidates {
                    res.push(Arc::clone(candidate));
                }
            }
        }

        Ok(res)
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

    // Close cleans up the Agent
    pub async fn close(&self) -> Result<(), Error> {
        if let Some(gather_candidate_cancel) = &self.gather_candidate_cancel {
            gather_candidate_cancel();
        }

        let mut ai = self.agent_internal.lock().await;
        ai.close().await
    }

    // set_remote_credentials sets the credentials of the remote agent
    pub async fn set_remote_credentials(
        &self,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<(), Error> {
        let mut ai = self.agent_internal.lock().await;
        ai.set_remote_credentials(remote_ufrag, remote_pwd)
    }

    // Restart restarts the ICE Agent with the provided ufrag/pwd
    // If no ufrag/pwd is provided the Agent will generate one itself
    //
    // Restart must only be called when GatheringState is GatheringStateComplete
    // a user must then call GatherCandidates explicitly to start generating new ones
    pub async fn restart(&self, mut ufrag: String, mut pwd: String) -> Result<(), Error> {
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

        if GatheringState::from(self.gathering_state.load(Ordering::SeqCst))
            == GatheringState::Gathering
        {
            return Err(ERR_RESTART_WHEN_GATHERING.to_owned());
        }
        self.gathering_state
            .store(GatheringState::New as u8, Ordering::SeqCst);

        let mut ai = self.agent_internal.lock().await;

        if ai.done_tx.is_none() {
            return Err(ERR_CLOSED.to_owned());
        }

        // Clear all agent needed to take back to fresh state
        ai.local_ufrag = ufrag;
        ai.local_pwd = pwd;
        ai.remote_ufrag = String::new();
        ai.remote_pwd = String::new();
        ai.pending_binding_requests = vec![];

        {
            let mut checklist = ai.agent_conn.checklist.lock().await;
            *checklist = vec![];
        }

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

    // GatherCandidates initiates the trickle based gathering process.
    pub async fn gather_candidates(&self) -> Result<(), Error> {
        if self.gathering_state.load(Ordering::SeqCst) != GatheringState::New as u8 {
            return Err(ERR_MULTIPLE_GATHER_ATTEMPTED.to_owned());
        }

        let chan_candidate_tx = {
            let ai = self.agent_internal.lock().await;
            if ai.on_candidate_hdlr.is_none() {
                return Err(ERR_NO_ON_CANDIDATE_HANDLER.to_owned());
            }
            ai.chan_candidate_tx.clone()
        };

        if let Some(gather_candidate_cancel) = &self.gather_candidate_cancel {
            gather_candidate_cancel(); // Cancel previous gathering routine
        }

        //TODO: a.gatherCandidateCancel = cancel

        let params = GatherCandidatesInternalParams {
            candidate_types: self.candidate_types.clone(),
            urls: self.urls.clone(),
            network_types: self.network_types.clone(),
            port_max: self.port_max,
            port_min: self.port_min,
            mdns_mode: self.mdns_mode,
            mdns_name: self.mdns_name.clone(),
            net: Arc::clone(&self.net),
            interface_filter: self.interface_filter.clone(),
            ext_ip_mapper: Arc::clone(&self.ext_ip_mapper),
            agent_internal: Arc::clone(&self.agent_internal),
            gathering_state: Arc::clone(&self.gathering_state),
            chan_candidate_tx,
        };
        tokio::spawn(async move {
            Agent::gather_candidates_internal(params).await;
        });

        Ok(())
    }

    // get_candidate_pairs_stats returns a list of candidate pair stats
    pub async fn get_candidate_pairs_stats(&self) -> Vec<CandidatePairStats> {
        let ai = self.agent_internal.lock().await;
        ai.get_candidate_pairs_stats().await
    }

    // get_local_candidates_stats returns a list of local candidates stats
    pub async fn get_local_candidates_stats(&self) -> Vec<CandidateStats> {
        let ai = self.agent_internal.lock().await;
        ai.get_local_candidates_stats()
    }

    // get_remote_candidates_stats returns a list of remote candidates stats
    pub async fn get_remote_candidates_stats(&self) -> Vec<CandidateStats> {
        let ai = self.agent_internal.lock().await;
        ai.get_remote_candidates_stats()
    }

    // unmarshal_remote_candidate creates a Remote Candidate from its string representation
    pub async fn unmarshal_remote_candidate(&self, raw: String) -> Result<impl Candidate, Error> {
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

                tcp_type = TcpType::from(split2[1]);
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
                config
                    .new_candidate_host(Some(Arc::clone(&self.agent_internal)))
                    .await
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
                    .new_candidate_server_reflexive(Some(Arc::clone(&self.agent_internal)))
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
                    .new_candidate_peer_reflexive(Some(Arc::clone(&self.agent_internal)))
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
                config
                    .new_candidate_relay(Some(Arc::clone(&self.agent_internal)))
                    .await
            }
            _ => Err(Error::new(format!(
                "{} ({})",
                *ERR_UNKNOWN_CANDIDATE_TYPE, typ
            ))),
        }
    }

    async fn resolve_and_add_multicast_candidate(
        mdns_conn: Arc<DNSConn>,
        c: Arc<dyn Candidate + Send + Sync>,
    ) -> Result<Arc<dyn Candidate + Send + Sync>, Error> {
        //TODO: hook up _close_query_signal_tx to Agent or Candidate's Close signal?
        let (_close_query_signal_tx, close_query_signal_rx) = mpsc::channel(1);
        let src = match mdns_conn.query(&c.address(), close_query_signal_rx).await {
            Ok((_, src)) => src,
            Err(err) => {
                log::warn!("Failed to discover mDNS candidate {}: {}", c.address(), err);
                return Err(err);
            }
        };

        c.set_ip(&src.ip()).await?;

        Ok(c)
    }

    async fn close_multicast_conn(mdns_conn: &Option<Arc<DNSConn>>) {
        if let Some(conn) = mdns_conn {
            if let Err(err) = conn.close().await {
                log::warn!("failed to close mDNS Conn: {}", err);
            }
        }
    }
}
