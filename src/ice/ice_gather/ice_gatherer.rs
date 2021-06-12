use crate::api::setting_engine::SettingEngine;
use crate::error::Error;
use crate::ice::ice_gather::ice_gatherer_state::ICEGathererState;
use crate::policy::ice_transport_policy::ICETransportPolicy;
use ice::candidate::{Candidate, CandidateType};

use crate::ice::ice_candidate_type::ICECandidateType;
use ice::agent::Agent;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub type OnLocalCandidateHdlrFn = Box<
    dyn (FnMut(&(dyn Candidate + Send + Sync)) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnStateChangeHdlrFn = Box<
    dyn (FnMut(ICEGathererState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnGatheringCompleteHdlrFn =
    Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

/// ICEGatherer gathers local host, server reflexive and relay
/// candidates, as well as enabling the retrieval of local Interactive
/// Connectivity Establishment (ICE) parameters which can be
/// exchanged in signaling.
pub struct ICEGatherer {
    state: AtomicU8, //ICEGathererState,

    validated_servers: Vec<ice::url::Url>,
    gather_policy: ICETransportPolicy,

    agent: Option<Arc<ice::agent::Agent>>,

    on_local_candidate_handler: Mutex<Option<OnLocalCandidateHdlrFn>>,
    on_state_change_handler: Mutex<Option<OnStateChangeHdlrFn>>,

    // Used for GatheringCompletePromise
    on_gathering_complete_handler: Mutex<Option<OnGatheringCompleteHdlrFn>>,

    setting_engine: SettingEngine, //TODO: api *API
}

impl ICEGatherer {
    pub(crate) async fn create_agent(&mut self) -> Result<(), Error> {
        if self.agent.is_some() || self.state() != ICEGathererState::New {
            return Ok(());
        }

        let mut candidate_types = vec![];
        if self.setting_engine.candidates.ice_lite {
            candidate_types.push(ice::candidate::CandidateType::Host);
        } else if self.gather_policy == ICETransportPolicy::Relay {
            candidate_types.push(ice::candidate::CandidateType::Relay);
        }

        let nat_1to1_cand_type = match self.setting_engine.candidates.nat_1to1_ip_candidate_type {
            ICECandidateType::Host => CandidateType::Host,
            ICECandidateType::Srflx => CandidateType::ServerReflexive,
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
            disconnected_timeout: Some(self.setting_engine.timeout.ice_disconnected_timeout),
            failed_timeout: Some(self.setting_engine.timeout.ice_failed_timeout),
            keepalive_interval: Some(self.setting_engine.timeout.ice_keepalive_interval),
            //LoggerFactory:          self.setting_engine.LoggerFactory,
            candidate_types,
            host_acceptance_min_wait: Some(
                self.setting_engine.timeout.ice_host_acceptance_min_wait,
            ),
            srflx_acceptance_min_wait: Some(
                self.setting_engine.timeout.ice_srflx_acceptance_min_wait,
            ),
            prflx_acceptance_min_wait: Some(
                self.setting_engine.timeout.ice_prflx_acceptance_min_wait,
            ),
            relay_acceptance_min_wait: Some(
                self.setting_engine.timeout.ice_relay_acceptance_min_wait,
            ),
            interface_filter: self.setting_engine.candidates.interface_filter.take(),
            nat_1to1_ips: self.setting_engine.candidates.nat_1to1_ips.clone(),
            nat_1to1_ip_candidate_type: nat_1to1_cand_type,
            net: self.setting_engine.net.take(),
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

        let agent = ice::agent::Agent::new(config).await?;
        self.agent = Some(Arc::new(agent));

        Ok(())
    }

    /// Gather ICE candidates.
    pub async fn gather(&mut self) -> Result<(), Error> {
        self.create_agent().await?;
        self.set_state(ICEGathererState::Gathering);

        if let Some(agent) = &self.agent {
            agent
                .on_candidate(Box::new(
                    move |_c: Option<Arc<dyn Candidate + Send + Sync>>| {
                        Box::pin(async move {
                            /*TODO: on_local_candidate_handler: = func(*ICECandidate)
                            {}
                            if handler, ok: = g.on_local_candidate_handler.Load().(func(candidate * ICECandidate));
                            ok && handler != nil {
                                on_local_candidate_handler = handler
                            }

                            on_gathering_complete_handler: = func()
                            {}
                            if handler, ok: = g.on_gathering_complete_handler.Load().(func());
                            ok && handler != nil {
                                on_gathering_complete_handler = handler
                            }

                            if candidate != nil {
                                c, err: = newICECandidateFromICE(candidate)
                                if err != nil {
                                    g.log.Warnf("Failed to convert ice.Candidate: %s", err)
                                    return
                                }
                                on_local_candidate_handler(&c)
                            } else {
                                g.setState(ICEGathererStateComplete)

                                on_gathering_complete_handler()
                                on_local_candidate_handler(nil)
                            }*/
                        })
                    },
                ))
                .await;

            agent.gather_candidates().await?;
        }

        Ok(())
    }
    /*
           // Close prunes all local candidates, and closes the ports.
           func (g *ICEGatherer) Close() error {
               g.lock.Lock()
               defer g.lock.Unlock()

               if g.agent == nil {
                   return nil
               } else if err := g.agent.Close(); err != nil {
                   return err
               }

               g.agent = nil
               g.setState(ICEGathererStateClosed)

               return nil
           }

           // GetLocalParameters returns the ICE parameters of the ICEGatherer.
           func (g *ICEGatherer) GetLocalParameters() (ICEParameters, error) {
               if err := g.createAgent(); err != nil {
                   return ICEParameters{}, err
               }

               frag, pwd, err := g.agent.GetLocalUserCredentials()
               if err != nil {
                   return ICEParameters{}, err
               }

               return ICEParameters{
                   UsernameFragment: frag,
                   Password:         pwd,
                   ICELite:          false,
               }, nil
           }


       /// GetLocalCandidates returns the sequence of valid local candidates associated with the ICEGatherer.
       pub fn GetLocalCandidates(&mut self) -> Result<Vec<Arc<dyn Candidate + Send + Sync>>, Error> {
           self.create_agent().await?;

           let iceCandidates = if let Some(agent) = &self.agent {
               agent.get_local_candidates().await?
           } else {
               vec![]
           };

           newICECandidatesFromICE(iceCandidates)
       }
    */
    /// on_local_candidate sets an event handler which fires when a new local ICE candidate is available
    /// Take note that the handler is gonna be called with a nil pointer when gathering is finished.
    pub async fn on_local_candidate(&self, f: OnLocalCandidateHdlrFn) {
        let mut on_local_candidate_handler = self.on_local_candidate_handler.lock().await;
        *on_local_candidate_handler = Some(f);
    }

    /// on_state_change fires any time the ICEGatherer changes
    pub async fn on_state_change(&self, f: OnStateChangeHdlrFn) {
        let mut on_state_change_handler = self.on_state_change_handler.lock().await;
        *on_state_change_handler = Some(f);
    }

    /// State indicates the current state of the ICE gatherer.
    pub fn state(&self) -> ICEGathererState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub(crate) fn set_state(&self, s: ICEGathererState) {
        self.state.store(s as u8, Ordering::SeqCst);

        /*TODO: if handler, ok := g.on_state_change_handler.Load().(func(state ICEGathererState)); ok && handler != nil {
            handler(s)
        }*/
    }

    pub(crate) fn get_agent(&self) -> Option<Arc<Agent>> {
        self.agent.clone()
    }

    /*TODO:
    func (g *ICEGatherer) collectStats(collector *statsReportCollector) {
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
