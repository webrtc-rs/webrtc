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
use crate::error::*;
use crate::external_ip_mapper::*;
use crate::mdns::*;
use crate::network_type::*;
use crate::state::*;
use crate::url::*;
use agent_config::*;
use agent_internal::*;
use agent_stats::*;

use anyhow::Result;
use mdns::conn::*;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use stun::{agent::*, attributes::*, fingerprint::*, integrity::*, message::*, xoraddr::*};
use util::{vnet::net::*, Buffer};

use crate::agent::agent_gather::GatherCandidatesInternalParams;
use crate::rand::*;
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
    pub(crate) agent_internal: Arc<Mutex<AgentInternal>>,

    pub(crate) port_min: u16,
    pub(crate) port_max: u16,
    pub(crate) interface_filter: Arc<Option<InterfaceFilterFn>>,
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
        if config.port_max < config.port_min {
            return Err(Error::ErrPort.into());
        }

        let mut mdns_name = config.multicast_dns_host_name.clone();
        if mdns_name.is_empty() {
            mdns_name = generate_multicast_dns_name();
        }

        if !mdns_name.ends_with(".local") || mdns_name.split('.').count() != 2 {
            return Err(Error::ErrInvalidMulticastDnshostName.into());
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
            return Err(Error::ErrLiteUsingNonHostCandidates.into());
        }

        if !config.urls.is_empty()
            && !contains_candidate_type(CandidateType::ServerReflexive, &candidate_types)
            && !contains_candidate_type(CandidateType::Relay, &candidate_types)
        {
            Self::close_multicast_conn(&mdns_conn).await;
            return Err(Error::ErrUselessUrlsProvided.into());
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

        let a = Self {
            port_min: config.port_min,
            port_max: config.port_max,
            agent_internal: Arc::new(Mutex::new(ai)),
            interface_filter: Arc::clone(&config.interface_filter),
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

        let agent_internal = Arc::clone(&a.agent_internal);

        Self::start_on_connection_state_change_routine(
            agent_internal,
            chan_state_rx,
            chan_candidate_rx,
            chan_candidate_pair_rx,
        )
        .await;

        // Restart is also used to initialize the agent for the first time
        if let Err(err) = a.restart(config.local_ufrag, config.local_pwd).await {
            Self::close_multicast_conn(&a.mdns_conn).await;
            let _ = a.close().await;
            return Err(err);
        }

        Ok(a)
    }

    /// Sets a handler that is fired when the connection state changes.
    pub async fn on_connection_state_change(&self, f: OnConnectionStateChangeHdlrFn) {
        let ai = self.agent_internal.lock().await;
        let mut on_connection_state_change_hdlr = ai.on_connection_state_change_hdlr.lock().await;
        *on_connection_state_change_hdlr = Some(f);
    }

    /// Sets a handler that is fired when the final candidate pair is selected.
    pub async fn on_selected_candidate_pair_change(&self, f: OnSelectedCandidatePairChangeHdlrFn) {
        let ai = self.agent_internal.lock().await;
        let mut on_selected_candidate_pair_change_hdlr =
            ai.on_selected_candidate_pair_change_hdlr.lock().await;
        *on_selected_candidate_pair_change_hdlr = Some(f);
    }

    /// Sets a handler that is fired when new candidates gathered. When the gathering process
    /// complete the last candidate is nil.
    pub async fn on_candidate(&self, f: OnCandidateHdlrFn) {
        let ai = self.agent_internal.lock().await;
        let mut on_candidate_hdlr = ai.on_candidate_hdlr.lock().await;
        *on_candidate_hdlr = Some(f);
    }

    async fn start_on_connection_state_change_routine(
        agent_internal: Arc<Mutex<AgentInternal>>,
        mut chan_state_rx: mpsc::Receiver<ConnectionState>,
        mut chan_candidate_rx: mpsc::Receiver<Option<Arc<dyn Candidate + Send + Sync>>>,
        mut chan_candidate_pair_rx: mpsc::Receiver<()>,
    ) {
        log::trace!("enter start_on_connection_state_change_routine");
        let agent_internal_pair = Arc::clone(&agent_internal);
        tokio::spawn(async move {
            // CandidatePair and ConnectionState are usually changed at once.
            // Blocking one by the other one causes deadlock.
            while chan_candidate_pair_rx.recv().await.is_some() {
                log::trace!("start_on_connection_state_change_routine: enter chan_candidate_pair_rx.recv before lock");
                let ai = agent_internal_pair.lock().await;
                log::trace!("start_on_connection_state_change_routine: enter chan_candidate_pair_rx.recv after lock");
                let selected_pair = {
                    let selected_pair = ai.agent_conn.selected_pair.lock().await;
                    selected_pair.clone()
                };

                {
                    let mut on_selected_candidate_pair_change_hdlr =
                        ai.on_selected_candidate_pair_change_hdlr.lock().await;
                    if let (Some(f), Some(p)) =
                        (&mut *on_selected_candidate_pair_change_hdlr, &selected_pair)
                    {
                        f(&p.local, &p.remote).await;
                    }
                }
                log::trace!(
                    "start_on_connection_state_change_routine: exit chan_candidate_pair_rx.recv"
                );
            }
        });

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    opt_state = chan_state_rx.recv() => {
                        log::trace!("start_on_connection_state_change_routine: enter chan_state_rx.recv before lock");
                        let ai = agent_internal.lock().await;
                        log::trace!("start_on_connection_state_change_routine: enter chan_state_rx.recv after lock");
                        if let Some(s) = opt_state {
                            let mut on_connection_state_change_hdlr = ai.on_connection_state_change_hdlr.lock().await;
                            if let Some(f) = &mut *on_connection_state_change_hdlr{
                                f(s).await;
                            }
                        } else {
                            while let Some(c) = chan_candidate_rx.recv().await {
                                let mut on_candidate_hdlr = ai.on_candidate_hdlr.lock().await;
                                if let Some(f) = &mut *on_candidate_hdlr {
                                    f(c).await;
                                }
                            }
                            break;
                        }
                        log::trace!("start_on_connection_state_change_routine: exit chan_state_rx.recv");
                    },
                    opt_cand = chan_candidate_rx.recv() => {
                        log::trace!("start_on_connection_state_change_routine: enter chan_candidate_rx.recv before lock");
                        let ai = agent_internal.lock().await;
                        log::trace!("start_on_connection_state_change_routine: enter chan_candidate_rx.recv after lock");
                        if let Some(c) = opt_cand {
                            let mut on_candidate_hdlr = ai.on_candidate_hdlr.lock().await;
                            if let Some(f) = &mut *on_candidate_hdlr{
                                f(c).await;
                            }
                        } else {
                            while let Some(s) = chan_state_rx.recv().await {
                                let mut on_connection_state_change_hdlr = ai.on_connection_state_change_hdlr.lock().await;
                                if let Some(f) = &mut *on_connection_state_change_hdlr{
                                    f(s).await;
                                }
                            }
                            break;
                        }
                        log::trace!("start_on_connection_state_change_routine: exit chan_candidate_rx.recv");
                    }
                }
            }
        });
    }

    /// Adds a new remote candidate.
    pub async fn add_remote_candidate(&self, c: &Arc<dyn Candidate + Send + Sync>) -> Result<()> {
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
                return Err(Error::ErrAddressParseFailed.into());
            }

            let agent_internal = Arc::clone(&self.agent_internal);
            let host_candidate = Arc::clone(c);
            let mdns_conn = self.mdns_conn.clone();
            tokio::spawn(async move {
                if let Some(mdns_conn) = mdns_conn {
                    if let Ok(candidate) =
                        Self::resolve_and_add_multicast_candidate(mdns_conn, host_candidate).await
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

    /// Returns the local candidates.
    pub async fn get_local_candidates(&self) -> Result<Vec<Arc<dyn Candidate + Send + Sync>>> {
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

    /// Returns the local user credentials.
    pub async fn get_local_user_credentials(&self) -> (String, String) {
        let ai = self.agent_internal.lock().await;
        (ai.local_ufrag.clone(), ai.local_pwd.clone())
    }

    /// Returns the remote user credentials.
    pub async fn get_remote_user_credentials(&self) -> (String, String) {
        let ai = self.agent_internal.lock().await;
        (ai.remote_ufrag.clone(), ai.remote_pwd.clone())
    }

    /// Cleans up the Agent.
    pub async fn close(&self) -> Result<()> {
        if let Some(gather_candidate_cancel) = &self.gather_candidate_cancel {
            gather_candidate_cancel();
        }

        //FIXME: deadlock here
        let mut ai = self.agent_internal.lock().await;
        ai.close().await
    }

    /// Returns the selected pair or nil if there is none
    pub async fn get_selected_candidate_pair(&self) -> Option<Arc<CandidatePair>> {
        let ai = self.agent_internal.lock().await;
        ai.agent_conn.get_selected_pair().await
    }

    /// Sets the credentials of the remote agent.
    pub async fn set_remote_credentials(
        &self,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<()> {
        let mut ai = self.agent_internal.lock().await;
        ai.set_remote_credentials(remote_ufrag, remote_pwd)
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
            return Err(Error::ErrLocalUfragInsufficientBits.into());
        }
        if pwd.len() * 8 < 128 {
            return Err(Error::ErrLocalPwdInsufficientBits.into());
        }

        if GatheringState::from(self.gathering_state.load(Ordering::SeqCst))
            == GatheringState::Gathering
        {
            return Err(Error::ErrRestartWhenGathering.into());
        }
        self.gathering_state
            .store(GatheringState::New as u8, Ordering::SeqCst);

        let mut ai = self.agent_internal.lock().await;

        {
            let done_tx = ai.done_tx.lock().await;
            if done_tx.is_none() {
                return Err(Error::ErrClosed.into());
            }
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
        ai.start().await;

        // Restart is used by NewAgent. Accept/Connect should be used to move to checking
        // for new Agents
        if ai.connection_state.load(Ordering::SeqCst) != ConnectionState::New as u8 {
            ai.update_connection_state(ConnectionState::Checking).await;
        }

        Ok(())
    }

    /// Initiates the trickle based gathering process.
    pub async fn gather_candidates(&self) -> Result<()> {
        if self.gathering_state.load(Ordering::SeqCst) != GatheringState::New as u8 {
            return Err(Error::ErrMultipleGatherAttempted.into());
        }

        let chan_candidate_tx = {
            let ai = self.agent_internal.lock().await;
            let on_candidate_hdlr = ai.on_candidate_hdlr.lock().await;
            if on_candidate_hdlr.is_none() {
                return Err(Error::ErrNoOnCandidateHandler.into());
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
            log::trace!("starting gather_candidates_internal");
            Self::gather_candidates_internal(params).await;
        });

        Ok(())
    }

    /// Returns a list of candidate pair stats.
    pub async fn get_candidate_pairs_stats(&self) -> Vec<CandidatePairStats> {
        let ai = self.agent_internal.lock().await;
        ai.get_candidate_pairs_stats().await
    }

    /// Returns a list of local candidates stats.
    pub async fn get_local_candidates_stats(&self) -> Vec<CandidateStats> {
        let ai = self.agent_internal.lock().await;
        ai.get_local_candidates_stats()
    }

    /// Returns a list of remote candidates stats.
    pub async fn get_remote_candidates_stats(&self) -> Vec<CandidateStats> {
        let ai = self.agent_internal.lock().await;
        ai.get_remote_candidates_stats()
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
                return Err(err);
            }
        };

        c.set_ip(&src.ip()).await?;

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
