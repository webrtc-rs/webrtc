use crate::api::setting_engine::SettingEngine;
use crate::error::{Error, Result};
use crate::ice_transport::ice_parameters::RTCIceParameters;
use crate::peer::ice::ice_candidate::ice_candidate_type::RTCIceCandidateType;
use crate::peer::ice::ice_candidate::*;
use crate::peer::ice::ice_gather::ice_gatherer_state::RTCIceGathererState;
use crate::peer::policy::ice_transport_policy::RTCIceTransportPolicy;

use ice::agent::Agent;
use ice::candidate::{Candidate, CandidateType};
use ice::url::Url;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

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

    pub(crate) on_local_candidate_handler: Arc<Mutex<Option<OnLocalCandidateHdlrFn>>>,
    pub(crate) on_state_change_handler: Arc<Mutex<Option<OnICEGathererStateChangeHdlrFn>>>,

    // Used for gathering_complete_promise
    pub(crate) on_gathering_complete_handler: Arc<Mutex<Option<OnGatheringCompleteHdlrFn>>>,
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
        {
            let agent = self.agent.lock().await;
            if agent.is_some() || self.state() != RTCIceGathererState::New {
                return Ok(());
            }
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

        let mut mdns_mode = self.setting_engine.candidates.multicast_dns_mode;
        if mdns_mode != ice::mdns::MulticastDnsMode::Disabled
            && mdns_mode != ice::mdns::MulticastDnsMode::QueryAndGather
        {
            // If enum is in state we don't recognized default to MulticastDNSModeQueryOnly
            mdns_mode = ice::mdns::MulticastDnsMode::QueryOnly;
        }

        let mut config = ice::agent::agent_config::AgentConfig {
            lite: self.setting_engine.candidates.ice_lite,
            urls: self.validated_servers.clone(),
            port_min: self.setting_engine.ephemeral_udp.port_min,
            port_max: self.setting_engine.ephemeral_udp.port_max,
            disconnected_timeout: self.setting_engine.timeout.ice_disconnected_timeout,
            failed_timeout: self.setting_engine.timeout.ice_failed_timeout,
            keepalive_interval: self.setting_engine.timeout.ice_keepalive_interval,
            candidate_types,
            host_acceptance_min_wait: self.setting_engine.timeout.ice_host_acceptance_min_wait,
            srflx_acceptance_min_wait: self.setting_engine.timeout.ice_srflx_acceptance_min_wait,
            prflx_acceptance_min_wait: self.setting_engine.timeout.ice_prflx_acceptance_min_wait,
            relay_acceptance_min_wait: self.setting_engine.timeout.ice_relay_acceptance_min_wait,
            interface_filter: self.setting_engine.candidates.interface_filter.clone(),
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
            //TODO: UDPMux:                 self.setting_engine.iceUDPMux,
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

        {
            let mut agent = self.agent.lock().await;
            *agent = Some(Arc::new(ice::agent::Agent::new(config).await?));
        }

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

            agent
                .on_candidate(Box::new(
                    move |candidate: Option<Arc<dyn Candidate + Send + Sync>>| {
                        let state_clone = Arc::clone(&state);
                        let on_local_candidate_handler_clone =
                            Arc::clone(&on_local_candidate_handler);
                        let on_state_change_handler_clone = Arc::clone(&on_state_change_handler);
                        let on_gathering_complete_handler_clone =
                            Arc::clone(&on_gathering_complete_handler);

                        Box::pin(async move {
                            if let Some(cand) = candidate {
                                let c = RTCIceCandidate::from(&cand);

                                let mut on_local_candidate_handler =
                                    on_local_candidate_handler_clone.lock().await;
                                if let Some(handler) = &mut *on_local_candidate_handler {
                                    handler(Some(c)).await;
                                }
                            } else {
                                state_clone
                                    .store(RTCIceGathererState::Complete as u8, Ordering::SeqCst);

                                {
                                    let mut on_state_change_handler =
                                        on_state_change_handler_clone.lock().await;
                                    if let Some(handler) = &mut *on_state_change_handler {
                                        handler(RTCIceGathererState::Complete).await;
                                    }
                                }

                                {
                                    let mut on_gathering_complete_handler =
                                        on_gathering_complete_handler_clone.lock().await;
                                    if let Some(handler) = &mut *on_gathering_complete_handler {
                                        handler().await;
                                    }
                                }

                                {
                                    let mut on_local_candidate_handler =
                                        on_local_candidate_handler_clone.lock().await;
                                    if let Some(handler) = &mut *on_local_candidate_handler {
                                        handler(None).await;
                                    }
                                }
                            }
                        })
                    },
                ))
                .await;

            agent.gather_candidates().await?;
        }

        Ok(())
    }

    /// Close prunes all local candidates, and closes the ports.
    pub async fn close(&self) -> Result<()> {
        let agent = {
            let mut agent_opt = self.agent.lock().await;
            agent_opt.take()
        };

        if let Some(agent) = agent {
            agent.close().await?;
        }
        self.set_state(RTCIceGathererState::Closed).await;

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
    pub async fn on_local_candidate(&self, f: OnLocalCandidateHdlrFn) {
        let mut on_local_candidate_handler = self.on_local_candidate_handler.lock().await;
        *on_local_candidate_handler = Some(f);
    }

    /// on_state_change sets an event handler which fires any time the ICEGatherer changes
    pub async fn on_state_change(&self, f: OnICEGathererStateChangeHdlrFn) {
        let mut on_state_change_handler = self.on_state_change_handler.lock().await;
        *on_state_change_handler = Some(f);
    }

    /// on_gathering_complete sets an event handler which fires any time the ICEGatherer changes
    pub async fn on_gathering_complete(&self, f: OnGatheringCompleteHdlrFn) {
        let mut on_gathering_complete_handler = self.on_gathering_complete_handler.lock().await;
        *on_gathering_complete_handler = Some(f);
    }

    /// State indicates the current state of the ICE gatherer.
    pub fn state(&self) -> RTCIceGathererState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub async fn set_state(&self, s: RTCIceGathererState) {
        self.state.store(s as u8, Ordering::SeqCst);

        let mut on_state_change_handler = self.on_state_change_handler.lock().await;
        if let Some(handler) = &mut *on_state_change_handler {
            handler(s).await;
        }
    }

    pub(crate) async fn get_agent(&self) -> Option<Arc<Agent>> {
        let agent = self.agent.lock().await;
        agent.clone()
    }

    /*TODO:func (g *ICEGatherer) collectStats(collector *statsReportCollector) {

        agent := g.getAgent()
        if agent == nil {
            return
        }

        collector.Collecting()
        go func(collector *statsReportCollector, agent *ice.Agent) {
            for _, candidatePairStats := range agent.GetCandidatePairsStats() {
                collector.Collecting()

                state, err := toStatsICECandidatePairState(candidatePairStats.State)
                if err != nil {
                    g.log.Error(err.Error())
                }

                pairID := newICECandidatePairStatsID(candidatePairStats.LocalCandidateID,
                    candidatePairStats.RemoteCandidateID)

                stats := ICECandidatePairStats{
                    Timestamp: statsTimestampFrom(candidatePairStats.Timestamp),
                    Type:      StatsTypeCandidatePair,
                    ID:        pairID,
                    // TransportID:
                    LocalCandidateID:            candidatePairStats.LocalCandidateID,
                    RemoteCandidateID:           candidatePairStats.RemoteCandidateID,
                    State:                       state,
                    Nominated:                   candidatePairStats.Nominated,
                    PacketsSent:                 candidatePairStats.PacketsSent,
                    PacketsReceived:             candidatePairStats.PacketsReceived,
                    BytesSent:                   candidatePairStats.BytesSent,
                    BytesReceived:               candidatePairStats.BytesReceived,
                    LastPacketSentTimestamp:     statsTimestampFrom(candidatePairStats.LastPacketSentTimestamp),
                    LastPacketReceivedTimestamp: statsTimestampFrom(candidatePairStats.LastPacketReceivedTimestamp),
                    FirstRequestTimestamp:       statsTimestampFrom(candidatePairStats.FirstRequestTimestamp),
                    LastRequestTimestamp:        statsTimestampFrom(candidatePairStats.LastRequestTimestamp),
                    LastResponseTimestamp:       statsTimestampFrom(candidatePairStats.LastResponseTimestamp),
                    TotalRoundTripTime:          candidatePairStats.TotalRoundTripTime,
                    CurrentRoundTripTime:        candidatePairStats.CurrentRoundTripTime,
                    AvailableOutgoingBitrate:    candidatePairStats.AvailableOutgoingBitrate,
                    AvailableIncomingBitrate:    candidatePairStats.AvailableIncomingBitrate,
                    CircuitBreakerTriggerCount:  candidatePairStats.CircuitBreakerTriggerCount,
                    RequestsReceived:            candidatePairStats.RequestsReceived,
                    RequestsSent:                candidatePairStats.RequestsSent,
                    ResponsesReceived:           candidatePairStats.ResponsesReceived,
                    ResponsesSent:               candidatePairStats.ResponsesSent,
                    RetransmissionsReceived:     candidatePairStats.RetransmissionsReceived,
                    RetransmissionsSent:         candidatePairStats.RetransmissionsSent,
                    ConsentRequestsSent:         candidatePairStats.ConsentRequestsSent,
                    ConsentExpiredTimestamp:     statsTimestampFrom(candidatePairStats.ConsentExpiredTimestamp),
                }
                collector.Collect(stats.ID, stats)
            }

            for _, candidateStats := range agent.GetLocalCandidatesStats() {
                collector.Collecting()

                networkType, err := getNetworkType(candidateStats.NetworkType)
                if err != nil {
                    g.log.Error(err.Error())
                }

                candidateType, err := get_candidate_type(candidateStats.CandidateType)
                if err != nil {
                    g.log.Error(err.Error())
                }

                stats := ICECandidateStats{
                    Timestamp:     statsTimestampFrom(candidateStats.Timestamp),
                    ID:            candidateStats.ID,
                    Type:          StatsTypeLocalCandidate,
                    NetworkType:   networkType,
                    IP:            candidateStats.IP,
                    Port:          int32(candidateStats.Port),
                    Protocol:      networkType.Protocol(),
                    CandidateType: candidateType,
                    priority:      int32(candidateStats.priority),
                    URL:           candidateStats.URL,
                    RelayProtocol: candidateStats.RelayProtocol,
                    Deleted:       candidateStats.Deleted,
                }
                collector.Collect(stats.ID, stats)
            }

            for _, candidateStats := range agent.GetRemoteCandidatesStats() {
                collector.Collecting()
                networkType, err := getNetworkType(candidateStats.NetworkType)
                if err != nil {
                    g.log.Error(err.Error())
                }

                candidateType, err := get_candidate_type(candidateStats.CandidateType)
                if err != nil {
                    g.log.Error(err.Error())
                }

                stats := ICECandidateStats{
                    Timestamp:     statsTimestampFrom(candidateStats.Timestamp),
                    ID:            candidateStats.ID,
                    Type:          StatsTypeRemoteCandidate,
                    NetworkType:   networkType,
                    IP:            candidateStats.IP,
                    Port:          int32(candidateStats.Port),
                    Protocol:      networkType.Protocol(),
                    CandidateType: candidateType,
                    priority:      int32(candidateStats.priority),
                    URL:           candidateStats.URL,
                    RelayProtocol: candidateStats.RelayProtocol,
                }
                collector.Collect(stats.ID, stats)
            }
            collector.Done()
        }(collector, agent)
    }
    */
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::api::APIBuilder;
    use crate::peer::ice::ice_gather::RTCIceGatherOptions;
    use crate::peer::ice::ice_server::RTCIceServer;
    use tokio::sync::mpsc;

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
        gatherer
            .on_local_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
                let gather_finished_tx_clone = Arc::clone(&gather_finished_tx);
                Box::pin(async move {
                    if c.is_none() {
                        let mut tx = gather_finished_tx_clone.lock().await;
                        tx.take();
                    }
                })
            }))
            .await;

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
        gatherer
            .on_local_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
                let done_tx_clone = Arc::clone(&done_tx);
                Box::pin(async move {
                    if let Some(c) = c {
                        if c.address.ends_with(".local") {
                            let mut tx = done_tx_clone.lock().await;
                            tx.take();
                        }
                    }
                })
            }))
            .await;

        gatherer.gather().await?;

        let _ = done_rx.recv().await;

        gatherer.close().await?;

        Ok(())
    }
}
