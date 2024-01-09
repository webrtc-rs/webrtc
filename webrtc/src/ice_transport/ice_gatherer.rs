use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use ice::agent::Agent;
use ice::candidate::{Candidate, CandidateType};
use ice::url::Url;
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
use util::{EventHandler, FutureUnit};

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

    gatherer_state: GathererState,
}

#[derive(Default, Clone)]
#[repr(transparent)]
struct GathererState {
    inner: Arc<GathererStateInner>,
}

#[derive(Default)]
struct GathererStateInner {
    state: Arc<AtomicU8>,
    event_handler: EventHandler<dyn InlineIceGathererEventHandler + Send + Sync>,
}

impl crate::ice::agent::AgentEventHandler for GathererState {
    fn on_candidate(
        &mut self,
        candidate: Option<Arc<dyn Candidate + Send + Sync>>,
    ) -> impl Future<Output = ()> + Send {
        async move {
            match (candidate, &*self.inner.event_handler.load()) {
                (Some(candidate), Some(handler)) => {
                    let cand = RTCIceCandidate::from(&candidate);
                    handler
                        .lock()
                        .await
                        .inline_on_local_candidate(Some(RTCIceCandidate::from(&candidate.clone())))
                        .await;
                }
                (_, maybe_handler) => {
                    self.inner
                        .state
                        .store(RTCIceGathererState::Complete as u8, Ordering::SeqCst);
                    if let Some(handler) = maybe_handler {
                        let mut handler = handler.lock().await;
                        handler
                            .inline_on_state_change(RTCIceGathererState::Complete)
                            .await;
                        handler.inline_on_gathering_complete().await;
                        handler.inline_on_local_candidate(None).await;
                    }
                }
            }
        }
    }
}

pub trait IceGathererEventHandler: Send {
    /// on_local_candidate sets an event handler which fires when a new local ICE candidate is available
    /// Take note that the handler is gonna be called with a nil pointer when gathering is finished.
    fn on_local_candidate(
        &mut self,
        candidate: Option<RTCIceCandidate>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// on_state_change sets an event handler which fires any time the ICEGatherer changes
    fn on_state_change(&mut self, state: RTCIceGathererState) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// on_gathering_complete sets an event handler which fires any time the ICEGatherer changes
    fn on_gathering_complete(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

trait InlineIceGathererEventHandler: Send {
    fn inline_on_local_candidate(&mut self, candidate: Option<RTCIceCandidate>) -> FutureUnit<'_>;
    fn inline_on_state_change(&mut self, state: RTCIceGathererState) -> FutureUnit<'_>;
    fn inline_on_gathering_complete(&mut self) -> FutureUnit<'_>;
}

impl<T> InlineIceGathererEventHandler for T
where
    T: IceGathererEventHandler,
{
    fn inline_on_local_candidate(&mut self, candidate: Option<RTCIceCandidate>) -> FutureUnit<'_> {
        FutureUnit::from_async(async move { self.on_local_candidate(candidate).await })
    }
    fn inline_on_state_change(&mut self, state: RTCIceGathererState) -> FutureUnit<'_> {
        FutureUnit::from_async(async move { self.on_state_change(state).await })
    }

    fn inline_on_gathering_complete(&mut self) -> FutureUnit<'_> {
        FutureUnit::from_async(async move { self.on_gathering_complete().await })
    }
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
            agent.with_event_handler(self.gatherer_state.clone());
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

    pub fn with_event_handler(
        &self,
        handler: impl InlineIceGathererEventHandler + Send + Sync + 'static,
    ) {
        self.gatherer_state
            .inner
            .event_handler
            .store(Box::new(handler))
    }

    /// State indicates the current state of the ICE gatherer.
    pub fn state(&self) -> RTCIceGathererState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub async fn set_state(&self, s: RTCIceGathererState) {
        self.state.store(s as u8, Ordering::SeqCst);

        if let Some(handler) = &*self.gatherer_state.inner.event_handler.load() {
            let mut handler = handler.lock().await;
            handler.inline_on_state_change(s).await;
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
