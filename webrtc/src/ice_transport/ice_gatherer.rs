use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use ice::agent::Agent;
use ice::candidate::{Candidate, CandidateType};
use ice::url::Url;
use portable_atomic::AtomicU8;
use tokio::sync::Mutex;

use crate::api::setting_engine::SettingEngine;
use crate::error::{Error, Result};
use crate::ice_transport::ice_candidate::*;
use crate::ice_transport::ice_candidate_type::RTCIceCandidateType;
use crate::ice_transport::ice_gatherer_state::RTCIceGathererState;
use crate::ice_transport::ice_parameters::RTCIceParameters;
use crate::ice_transport::ice_server::RTCIceServer;
use crate::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy;
use crate::stats::stats_collector::StatsCollector;
use crate::stats::SourceStatsType::*;
use crate::stats::{ICECandidatePairStats, StatsReportType};

/// ICEGatherOptions provides options relating to the gathering of ICE candidates.
#[derive(Default, Debug, Clone)]
pub struct RTCIceGatherOptions {
    pub ice_servers: Vec<RTCIceServer>,
    pub ice_gather_policy: RTCIceTransportPolicy,
}

pub type OnLocalCandidateHdlrFn = Box<
    dyn (FnMut(Option<RTCIceCandidate>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnICEGathererStateChangeHdlrFn = Box<
    dyn (FnMut(RTCIceGathererState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnGatheringCompleteHdlrFn =
    Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

/// ICEGatherer gathers local host, server reflexive and relay
/// candidates, as well as enabling the retrieval of local Interactive
/// Connectivity Establishment (ICE) parameters which can be
/// exchanged in signaling.
#[derive(Default)]
pub struct RTCIceGatherer {
    pub(crate) validated_servers: Vec<Url>,
    pub(crate) gather_policy: RTCIceTransportPolicy,
    pub(crate) setting_engine: Arc<SettingEngine>,

    pub(crate) state: Arc<AtomicU8>, //ICEGathererState,
    pub(crate) agent: Mutex<Option<Arc<ice::agent::Agent>>>,

    pub(crate) on_local_candidate_handler: Arc<ArcSwapOption<Mutex<OnLocalCandidateHdlrFn>>>,
    pub(crate) on_state_change_handler: Arc<ArcSwapOption<Mutex<OnICEGathererStateChangeHdlrFn>>>,

    // Used for gathering_complete_promise
    pub(crate) on_gathering_complete_handler: Arc<ArcSwapOption<Mutex<OnGatheringCompleteHdlrFn>>>,
}

impl RTCIceGatherer {
    pub(crate) fn new(
        validated_servers: Vec<Url>,
        gather_policy: RTCIceTransportPolicy,
        setting_engine: Arc<SettingEngine>,
    ) -> Self {
        RTCIceGatherer {
            gather_policy,
            validated_servers,
            setting_engine,
            state: Arc::new(AtomicU8::new(RTCIceGathererState::New as u8)),
            ..Default::default()
        }
    }

    pub(crate) async fn create_agent(&self) -> Result<()> {
        // NOTE: A lock is held for the duration of this function in order to
        // avoid potential double-agent creations. Care should be taken to
        // ensure we do not do anything expensive other than the actual agent
        // creation in this function.
        let mut agent = self.agent.lock().await;

        if agent.is_some() || self.state() != RTCIceGathererState::New {
            return Ok(());
        }

        let mut candidate_types = vec![];
        if self.setting_engine.candidates.ice_lite {
            candidate_types.push(ice::candidate::CandidateType::Host);
        } else if self.gather_policy == RTCIceTransportPolicy::Relay {
            candidate_types.push(ice::candidate::CandidateType::Relay);
        }

        let nat_1to1_cand_type = match self.setting_engine.candidates.nat_1to1_ip_candidate_type {
            RTCIceCandidateType::Host => CandidateType::Host,
            RTCIceCandidateType::Srflx => CandidateType::ServerReflexive,
            _ => CandidateType::Unspecified,
        };

        let mdns_mode = self.setting_engine.candidates.multicast_dns_mode;

        let mut config = ice::agent::agent_config::AgentConfig {
            udp_network: self.setting_engine.udp_network.clone(),
            lite: self.setting_engine.candidates.ice_lite,
            urls: self.validated_servers.clone(),
            disconnected_timeout: self.setting_engine.timeout.ice_disconnected_timeout,
            failed_timeout: self.setting_engine.timeout.ice_failed_timeout,
            keepalive_interval: self.setting_engine.timeout.ice_keepalive_interval,
            candidate_types,
            host_acceptance_min_wait: self.setting_engine.timeout.ice_host_acceptance_min_wait,
            srflx_acceptance_min_wait: self.setting_engine.timeout.ice_srflx_acceptance_min_wait,
            prflx_acceptance_min_wait: self.setting_engine.timeout.ice_prflx_acceptance_min_wait,
            relay_acceptance_min_wait: self.setting_engine.timeout.ice_relay_acceptance_min_wait,
            interface_filter: self.setting_engine.candidates.interface_filter.clone(),
            ip_filter: self.setting_engine.candidates.ip_filter.clone(),
            nat_1to1_ips: self.setting_engine.candidates.nat_1to1_ips.clone(),
            nat_1to1_ip_candidate_type: nat_1to1_cand_type,
            include_loopback: self.setting_engine.candidates.include_loopback_candidate,
            net: self.setting_engine.vnet.clone(),
            multicast_dns_mode: mdns_mode,
            multicast_dns_host_name: self
                .setting_engine
                .candidates
                .multicast_dns_host_name
                .clone(),
            local_ufrag: self.setting_engine.candidates.username_fragment.clone(),
            local_pwd: self.setting_engine.candidates.password.clone(),
            //TODO: TCPMux:                 self.setting_engine.iceTCPMux,
            //TODO: ProxyDialer:            self.setting_engine.iceProxyDialer,
            ..Default::default()
        };

        let requested_network_types = if self.setting_engine.candidates.ice_network_types.is_empty()
        {
            ice::network_type::supported_network_types()
        } else {
            self.setting_engine.candidates.ice_network_types.clone()
        };

        config.network_types.extend(requested_network_types);

        *agent = Some(Arc::new(ice::agent::Agent::new(config).await?));

        Ok(())
    }

    /// Gather ICE candidates.
    pub async fn gather(&self) -> Result<()> {
        self.create_agent().await?;
        self.set_state(RTCIceGathererState::Gathering).await;

        if let Some(agent) = self.get_agent().await {
            let state = Arc::clone(&self.state);
            let on_local_candidate_handler = Arc::clone(&self.on_local_candidate_handler);
            let on_state_change_handler = Arc::clone(&self.on_state_change_handler);
            let on_gathering_complete_handler = Arc::clone(&self.on_gathering_complete_handler);

            agent.on_candidate(Box::new(
                move |candidate: Option<Arc<dyn Candidate + Send + Sync>>| {
                    let state_clone = Arc::clone(&state);
                    let on_local_candidate_handler_clone = Arc::clone(&on_local_candidate_handler);
                    let on_state_change_handler_clone = Arc::clone(&on_state_change_handler);
                    let on_gathering_complete_handler_clone =
                        Arc::clone(&on_gathering_complete_handler);

                    Box::pin(async move {
                        if let Some(cand) = candidate {
                            if let Some(handler) = &*on_local_candidate_handler_clone.load() {
                                let mut f = handler.lock().await;
                                f(Some(RTCIceCandidate::from(&cand))).await;
                            }
                        } else {
                            state_clone
                                .store(RTCIceGathererState::Complete as u8, Ordering::SeqCst);

                            if let Some(handler) = &*on_state_change_handler_clone.load() {
                                let mut f = handler.lock().await;
                                f(RTCIceGathererState::Complete).await;
                            }

                            if let Some(handler) = &*on_gathering_complete_handler_clone.load() {
                                let mut f = handler.lock().await;
                                f().await;
                            }

                            if let Some(handler) = &*on_local_candidate_handler_clone.load() {
                                let mut f = handler.lock().await;
                                f(None).await;
                            }
                        }
                    })
                },
            ));

            agent.gather_candidates()?;
        }

        Ok(())
    }

    /// Close prunes all local candidates, and closes the ports.
    pub async fn close(&self) -> Result<()> {
        self.set_state(RTCIceGathererState::Closed).await;

        let agent = {
            let mut agent_opt = self.agent.lock().await;
            agent_opt.take()
        };

        if let Some(agent) = agent {
            agent.close().await?;
        }

        Ok(())
    }

    /// get_local_parameters returns the ICE parameters of the ICEGatherer.
    pub async fn get_local_parameters(&self) -> Result<RTCIceParameters> {
        self.create_agent().await?;

        let (frag, pwd) = if let Some(agent) = self.get_agent().await {
            agent.get_local_user_credentials().await
        } else {
            return Err(Error::ErrICEAgentNotExist);
        };

        Ok(RTCIceParameters {
            username_fragment: frag,
            password: pwd,
            ice_lite: false,
        })
    }

    /// get_local_candidates returns the sequence of valid local candidates associated with the ICEGatherer.
    pub async fn get_local_candidates(&self) -> Result<Vec<RTCIceCandidate>> {
        self.create_agent().await?;

        let ice_candidates = if let Some(agent) = self.get_agent().await {
            agent.get_local_candidates().await?
        } else {
            return Err(Error::ErrICEAgentNotExist);
        };

        Ok(rtc_ice_candidates_from_ice_candidates(&ice_candidates))
    }

    /// on_local_candidate sets an event handler which fires when a new local ICE candidate is available
    /// Take note that the handler is gonna be called with a nil pointer when gathering is finished.
    pub fn on_local_candidate(&self, f: OnLocalCandidateHdlrFn) {
        self.on_local_candidate_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    /// on_state_change sets an event handler which fires any time the ICEGatherer changes
    pub fn on_state_change(&self, f: OnICEGathererStateChangeHdlrFn) {
        self.on_state_change_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    /// on_gathering_complete sets an event handler which fires any time the ICEGatherer changes
    pub fn on_gathering_complete(&self, f: OnGatheringCompleteHdlrFn) {
        self.on_gathering_complete_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    /// State indicates the current state of the ICE gatherer.
    pub fn state(&self) -> RTCIceGathererState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub async fn set_state(&self, s: RTCIceGathererState) {
        self.state.store(s as u8, Ordering::SeqCst);

        if let Some(handler) = &*self.on_state_change_handler.load() {
            let mut f = handler.lock().await;
            f(s).await;
        }
    }

    pub(crate) async fn get_agent(&self) -> Option<Arc<Agent>> {
        let agent = self.agent.lock().await;
        agent.clone()
    }

    pub(crate) async fn collect_stats(&self, collector: &StatsCollector) {
        if let Some(agent) = self.get_agent().await {
            let mut reports = HashMap::new();

            for stats in agent.get_candidate_pairs_stats().await {
                let stats: ICECandidatePairStats = stats.into();
                reports.insert(stats.id.clone(), StatsReportType::CandidatePair(stats));
            }

            for stats in agent.get_local_candidates_stats().await {
                reports.insert(
                    stats.id.clone(),
                    StatsReportType::from(LocalCandidate(stats)),
                );
            }

            for stats in agent.get_remote_candidates_stats().await {
                reports.insert(
                    stats.id.clone(),
                    StatsReportType::from(RemoteCandidate(stats)),
                );
            }

            collector.merge(reports);
        }
    }
}

#[cfg(test)]
mod test {
    use tokio::sync::mpsc;

    use super::*;
    use crate::api::APIBuilder;
    use crate::ice_transport::ice_gatherer::RTCIceGatherOptions;
    use crate::ice_transport::ice_server::RTCIceServer;

    #[tokio::test]
    async fn test_new_ice_gatherer_success() -> Result<()> {
        let opts = RTCIceGatherOptions {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let gatherer = APIBuilder::new().build().new_ice_gatherer(opts)?;

        assert_eq!(
            gatherer.state(),
            RTCIceGathererState::New,
            "Expected gathering state new"
        );

        let (gather_finished_tx, mut gather_finished_rx) = mpsc::channel::<()>(1);
        let gather_finished_tx = Arc::new(Mutex::new(Some(gather_finished_tx)));
        gatherer.on_local_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
            let gather_finished_tx_clone = Arc::clone(&gather_finished_tx);
            Box::pin(async move {
                if c.is_none() {
                    let mut tx = gather_finished_tx_clone.lock().await;
                    tx.take();
                }
            })
        }));

        gatherer.gather().await?;

        let _ = gather_finished_rx.recv().await;

        let params = gatherer.get_local_parameters().await?;

        assert!(
            !params.username_fragment.is_empty() && !params.password.is_empty(),
            "Empty local username or password frag"
        );

        let candidates = gatherer.get_local_candidates().await?;

        assert!(!candidates.is_empty(), "No candidates gathered");

        gatherer.close().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_ice_gather_mdns_candidate_gathering() -> Result<()> {
        let mut s = SettingEngine::default();
        s.set_ice_multicast_dns_mode(ice::mdns::MulticastDnsMode::QueryAndGather);

        let gatherer = APIBuilder::new()
            .with_setting_engine(s)
            .build()
            .new_ice_gatherer(RTCIceGatherOptions::default())?;

        let (done_tx, mut done_rx) = mpsc::channel::<()>(1);
        let done_tx = Arc::new(Mutex::new(Some(done_tx)));
        gatherer.on_local_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
            let done_tx_clone = Arc::clone(&done_tx);
            Box::pin(async move {
                if let Some(c) = c {
                    if c.address.ends_with(".local") {
                        let mut tx = done_tx_clone.lock().await;
                        tx.take();
                    }
                }
            })
        }));

        gatherer.gather().await?;

        let _ = done_rx.recv().await;

        gatherer.close().await?;

        Ok(())
    }
}
