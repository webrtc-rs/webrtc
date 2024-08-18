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

use std::collections::HashMap;
use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;

use agent_config::*;
use agent_internal::*;
use agent_stats::*;
use mdns::conn::*;
use portable_atomic::{AtomicU8, AtomicUsize};
use stun::agent::*;
use stun::attributes::*;
use stun::fingerprint::*;
use stun::integrity::*;
use stun::message::*;
use stun::xoraddr::*;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::{Duration, Instant};
use util::vnet::net::*;
use util::Buffer;

use crate::agent::agent_gather::GatherCandidatesInternalParams;
use crate::candidate::*;
use crate::error::*;
use crate::external_ip_mapper::*;
use crate::mdns::*;
use crate::network_type::*;
use crate::rand::*;
use crate::state::*;
use crate::tcp_type::TcpType;
use crate::udp_mux::UDPMux;
use crate::udp_network::UDPNetwork;
use crate::url::*;

#[derive(Debug, Clone)]
pub(crate) struct BindingRequest {
    pub(crate) timestamp: Instant,
    pub(crate) transaction_id: TransactionId,
    pub(crate) destination: SocketAddr,
    pub(crate) is_use_candidate: bool,
}

impl Default for BindingRequest {
    fn default() -> Self {
        Self {
            timestamp: Instant::now(),
            transaction_id: TransactionId::default(),
            destination: SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
            is_use_candidate: false,
        }
    }
}

pub type OnConnectionStateChangeHdlrFn = Box<
    dyn (FnMut(ConnectionState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;
pub type OnSelectedCandidatePairChangeHdlrFn = Box<
    dyn (FnMut(
            &Arc<dyn Candidate + Send + Sync>,
            &Arc<dyn Candidate + Send + Sync>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;
pub type OnCandidateHdlrFn = Box<
    dyn (FnMut(
            Option<Arc<dyn Candidate + Send + Sync>>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;
pub type GatherCandidateCancelFn = Box<dyn Fn() + Send + Sync>;

struct ChanReceivers {
    chan_state_rx: mpsc::Receiver<ConnectionState>,
    chan_candidate_rx: mpsc::Receiver<Option<Arc<dyn Candidate + Send + Sync>>>,
    chan_candidate_pair_rx: mpsc::Receiver<()>,
}

/// Represents the ICE agent.
pub struct Agent {
    pub(crate) internal: Arc<AgentInternal>,

    pub(crate) udp_network: UDPNetwork,
    pub(crate) interface_filter: Arc<Option<InterfaceFilterFn>>,
    pub(crate) include_loopback: bool,
    pub(crate) ip_filter: Arc<Option<IpFilterFn>>,
    pub(crate) mdns_mode: MulticastDnsMode,
    pub(crate) mdns_name: String,
    pub(crate) mdns_conn: Option<Arc<DnsConn>>,
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
    /// Creates a new Agent.
    pub async fn new(config: AgentConfig) -> Result<Self> {
        let mut mdns_name = config.multicast_dns_host_name.clone();
        if mdns_name.is_empty() {
            mdns_name = generate_multicast_dns_name();
        }

        if !mdns_name.ends_with(".local") || mdns_name.split('.').count() != 2 {
            return Err(Error::ErrInvalidMulticastDnshostName);
        }

        let mdns_mode = config.multicast_dns_mode;

        let mdns_conn =
            match create_multicast_dns(mdns_mode, &mdns_name, &config.multicast_dns_dest_addr) {
                Ok(c) => c,
                Err(err) => {
                    // Opportunistic mDNS: If we can't open the connection, that's ok: we
                    // can continue without it.
                    log::warn!("Failed to initialize mDNS {}: {}", mdns_name, err);
                    None
                }
            };

        let (mut ai, chan_receivers) = AgentInternal::new(&config);
        let (chan_state_rx, chan_candidate_rx, chan_candidate_pair_rx) = (
            chan_receivers.chan_state_rx,
            chan_receivers.chan_candidate_rx,
            chan_receivers.chan_candidate_pair_rx,
        );

        config.init_with_defaults(&mut ai);

        let candidate_types = if config.candidate_types.is_empty() {
            default_candidate_types()
        } else {
            config.candidate_types.clone()
        };

        if ai.lite.load(Ordering::SeqCst)
            && (candidate_types.len() != 1 || candidate_types[0] != CandidateType::Host)
        {
            Self::close_multicast_conn(&mdns_conn).await;
            return Err(Error::ErrLiteUsingNonHostCandidates);
        }

        if !config.urls.is_empty()
            && !contains_candidate_type(CandidateType::ServerReflexive, &candidate_types)
            && !contains_candidate_type(CandidateType::Relay, &candidate_types)
        {
            Self::close_multicast_conn(&mdns_conn).await;
            return Err(Error::ErrUselessUrlsProvided);
        }

        let ext_ip_mapper = match config.init_ext_ip_mapping(mdns_mode, &candidate_types) {
            Ok(ext_ip_mapper) => ext_ip_mapper,
            Err(err) => {
                Self::close_multicast_conn(&mdns_conn).await;
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

        let agent = Self {
            udp_network: config.udp_network,
            internal: Arc::new(ai),
            interface_filter: Arc::clone(&config.interface_filter),
            include_loopback: config.include_loopback,
            ip_filter: Arc::clone(&config.ip_filter),
            mdns_mode,
            mdns_name,
            mdns_conn,
            net,
            ext_ip_mapper: Arc::new(ext_ip_mapper),
            gathering_state: Arc::new(AtomicU8::new(0)), //GatheringState::New,
            candidate_types,
            urls: config.urls.clone(),
            network_types: config.network_types.clone(),

            gather_candidate_cancel: None, //TODO: add cancel
        };

        agent.internal.start_on_connection_state_change_routine(
            chan_state_rx,
            chan_candidate_rx,
            chan_candidate_pair_rx,
        );

        // Restart is also used to initialize the agent for the first time
        if let Err(err) = agent.restart(config.local_ufrag, config.local_pwd).await {
            Self::close_multicast_conn(&agent.mdns_conn).await;
            let _ = agent.close().await;
            return Err(err);
        }

        Ok(agent)
    }

    pub fn get_bytes_received(&self) -> usize {
        self.internal.agent_conn.bytes_received()
    }

    pub fn get_bytes_sent(&self) -> usize {
        self.internal.agent_conn.bytes_sent()
    }

    /// Sets a handler that is fired when the connection state changes.
    pub fn on_connection_state_change(&self, f: OnConnectionStateChangeHdlrFn) {
        self.internal
            .on_connection_state_change_hdlr
            .store(Some(Arc::new(Mutex::new(f))))
    }

    /// Sets a handler that is fired when the final candidate pair is selected.
    pub fn on_selected_candidate_pair_change(&self, f: OnSelectedCandidatePairChangeHdlrFn) {
        self.internal
            .on_selected_candidate_pair_change_hdlr
            .store(Some(Arc::new(Mutex::new(f))))
    }

    /// Sets a handler that is fired when new candidates gathered. When the gathering process
    /// complete the last candidate is nil.
    pub fn on_candidate(&self, f: OnCandidateHdlrFn) {
        self.internal
            .on_candidate_hdlr
            .store(Some(Arc::new(Mutex::new(f))));
    }

    /// Adds a new remote candidate.
    pub fn add_remote_candidate(&self, c: &Arc<dyn Candidate + Send + Sync>) -> Result<()> {
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
                return Err(Error::ErrAddressParseFailed);
            }

            let ai = Arc::clone(&self.internal);
            let host_candidate = Arc::clone(c);
            let mdns_conn = self.mdns_conn.clone();
            tokio::spawn(async move {
                if let Some(mdns_conn) = mdns_conn {
                    if let Ok(candidate) =
                        Self::resolve_and_add_multicast_candidate(mdns_conn, host_candidate).await
                    {
                        ai.add_remote_candidate(&candidate).await;
                    }
                }
            });
        } else {
            let ai = Arc::clone(&self.internal);
            let candidate = Arc::clone(c);
            tokio::spawn(async move {
                ai.add_remote_candidate(&candidate).await;
            });
        }

        Ok(())
    }

    /// Returns the local candidates.
    pub async fn get_local_candidates(&self) -> Result<Vec<Arc<dyn Candidate + Send + Sync>>> {
        let mut res = vec![];

        {
            let local_candidates = self.internal.local_candidates.lock().await;
            for candidates in local_candidates.values() {
                for candidate in candidates {
                    res.push(Arc::clone(candidate));
                }
            }
        }

        Ok(res)
    }

    /// Returns the local user credentials.
    pub async fn get_local_user_credentials(&self) -> (String, String) {
        let ufrag_pwd = self.internal.ufrag_pwd.lock().await;
        (ufrag_pwd.local_ufrag.clone(), ufrag_pwd.local_pwd.clone())
    }

    /// Returns the remote user credentials.
    pub async fn get_remote_user_credentials(&self) -> (String, String) {
        let ufrag_pwd = self.internal.ufrag_pwd.lock().await;
        (ufrag_pwd.remote_ufrag.clone(), ufrag_pwd.remote_pwd.clone())
    }

    /// Cleans up the Agent.
    pub async fn close(&self) -> Result<()> {
        if let Some(gather_candidate_cancel) = &self.gather_candidate_cancel {
            gather_candidate_cancel();
        }

        if let UDPNetwork::Muxed(ref udp_mux) = self.udp_network {
            let (ufrag, _) = self.get_local_user_credentials().await;
            udp_mux.remove_conn_by_ufrag(&ufrag).await;
        }

        //FIXME: deadlock here
        self.internal.close().await
    }

    /// Returns the selected pair or nil if there is none
    pub fn get_selected_candidate_pair(&self) -> Option<Arc<CandidatePair>> {
        self.internal.agent_conn.get_selected_pair()
    }

    /// Sets the credentials of the remote agent.
    pub async fn set_remote_credentials(
        &self,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<()> {
        self.internal
            .set_remote_credentials(remote_ufrag, remote_pwd)
            .await
    }

    /// Restarts the ICE Agent with the provided ufrag/pwd
    /// If no ufrag/pwd is provided the Agent will generate one itself.
    ///
    /// Restart must only be called when `GatheringState` is `GatheringStateComplete`
    /// a user must then call `GatherCandidates` explicitly to start generating new ones.
    pub async fn restart(&self, mut ufrag: String, mut pwd: String) -> Result<()> {
        if ufrag.is_empty() {
            ufrag = generate_ufrag();
        }
        if pwd.is_empty() {
            pwd = generate_pwd();
        }

        if ufrag.len() * 8 < 24 {
            return Err(Error::ErrLocalUfragInsufficientBits);
        }
        if pwd.len() * 8 < 128 {
            return Err(Error::ErrLocalPwdInsufficientBits);
        }

        if GatheringState::from(self.gathering_state.load(Ordering::SeqCst))
            == GatheringState::Gathering
        {
            return Err(Error::ErrRestartWhenGathering);
        }
        self.gathering_state
            .store(GatheringState::New as u8, Ordering::SeqCst);

        {
            let done_tx = self.internal.done_tx.lock().await;
            if done_tx.is_none() {
                return Err(Error::ErrClosed);
            }
        }

        // Clear all agent needed to take back to fresh state
        {
            let mut ufrag_pwd = self.internal.ufrag_pwd.lock().await;
            ufrag_pwd.local_ufrag = ufrag;
            ufrag_pwd.local_pwd = pwd;
            ufrag_pwd.remote_ufrag = String::new();
            ufrag_pwd.remote_pwd = String::new();
        }
        {
            let mut pending_binding_requests = self.internal.pending_binding_requests.lock().await;
            *pending_binding_requests = vec![];
        }

        {
            let mut checklist = self.internal.agent_conn.checklist.lock().await;
            *checklist = vec![];
        }

        self.internal.set_selected_pair(None).await;
        self.internal.delete_all_candidates().await;
        self.internal.start().await;

        // Restart is used by NewAgent. Accept/Connect should be used to move to checking
        // for new Agents
        if self.internal.connection_state.load(Ordering::SeqCst) != ConnectionState::New as u8 {
            self.internal
                .update_connection_state(ConnectionState::Checking)
                .await;
        }

        Ok(())
    }

    /// Initiates the trickle based gathering process.
    pub fn gather_candidates(&self) -> Result<()> {
        if self.gathering_state.load(Ordering::SeqCst) != GatheringState::New as u8 {
            return Err(Error::ErrMultipleGatherAttempted);
        }

        if self.internal.on_candidate_hdlr.load().is_none() {
            return Err(Error::ErrNoOnCandidateHandler);
        }

        if let Some(gather_candidate_cancel) = &self.gather_candidate_cancel {
            gather_candidate_cancel(); // Cancel previous gathering routine
        }

        //TODO: a.gatherCandidateCancel = cancel

        let params = GatherCandidatesInternalParams {
            udp_network: self.udp_network.clone(),
            candidate_types: self.candidate_types.clone(),
            urls: self.urls.clone(),
            network_types: self.network_types.clone(),
            mdns_mode: self.mdns_mode,
            mdns_name: self.mdns_name.clone(),
            net: Arc::clone(&self.net),
            interface_filter: self.interface_filter.clone(),
            ip_filter: self.ip_filter.clone(),
            ext_ip_mapper: Arc::clone(&self.ext_ip_mapper),
            agent_internal: Arc::clone(&self.internal),
            gathering_state: Arc::clone(&self.gathering_state),
            chan_candidate_tx: Arc::clone(&self.internal.chan_candidate_tx),
            include_loopback: self.include_loopback,
        };
        tokio::spawn(async move {
            Self::gather_candidates_internal(params).await;
        });

        Ok(())
    }

    /// Returns a list of candidate pair stats.
    pub async fn get_candidate_pairs_stats(&self) -> Vec<CandidatePairStats> {
        self.internal.get_candidate_pairs_stats().await
    }

    /// Returns a list of local candidates stats.
    pub async fn get_local_candidates_stats(&self) -> Vec<CandidateStats> {
        self.internal.get_local_candidates_stats().await
    }

    /// Returns a list of remote candidates stats.
    pub async fn get_remote_candidates_stats(&self) -> Vec<CandidateStats> {
        self.internal.get_remote_candidates_stats().await
    }

    async fn resolve_and_add_multicast_candidate(
        mdns_conn: Arc<DnsConn>,
        c: Arc<dyn Candidate + Send + Sync>,
    ) -> Result<Arc<dyn Candidate + Send + Sync>> {
        //TODO: hook up _close_query_signal_tx to Agent or Candidate's Close signal?
        let (_close_query_signal_tx, close_query_signal_rx) = mpsc::channel(1);
        let src = match mdns_conn.query(&c.address(), close_query_signal_rx).await {
            Ok((_, src)) => src,
            Err(err) => {
                log::warn!("Failed to discover mDNS candidate {}: {}", c.address(), err);
                return Err(err.into());
            }
        };

        c.set_ip(&src.ip())?;

        Ok(c)
    }

    async fn close_multicast_conn(mdns_conn: &Option<Arc<DnsConn>>) {
        if let Some(conn) = mdns_conn {
            if let Err(err) = conn.close().await {
                log::warn!("failed to close mDNS Conn: {}", err);
            }
        }
    }
}
