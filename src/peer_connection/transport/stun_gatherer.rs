//! ICE Candidate Gathering (Sans-I/O)
//!
//! This module provides RTCStunGatherer for gathering ICE candidates in a Sans-I/O manner.
//! Unlike the old async version, this gatherer is a configuration object that holds
//! the ICE servers and state.

use crate::runtime::Runtime;
use rtc::ice::candidate::CandidateConfig;
use rtc::peer_connection::configuration::{RTCIceServer, RTCIceTransportPolicy};
use rtc::peer_connection::transport::{
    CandidateHostConfig, CandidateServerReflexiveConfig, RTCIceCandidate, RTCIceCandidateInit,
};
use rtc::sansio::Protocol;
use rtc::shared::error::Error;
use rtc::shared::{FourTuple, TaggedBytesMut, TransportProtocol};
use rtc::stun::{
    client::Client as StunClient, client::ClientBuilder as StunClientBuilder,
    client::TaggedMessage, message::BINDING_REQUEST, message::Message as StunMessage,
    message::TransactionId,
};
use std::sync::Arc;
/*use rtc::turn::client::{
    Client as TurnClient, ClientConfig as TurnClientConfig, Event as TurnEvent,
};*/
use log::{debug, error};
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::stun::agent::StunEvent;
use rtc::stun::message::Getter;
use rtc::stun::xoraddr::XorMappedAddress;
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::time::Instant;

#[derive(Debug)]
pub(crate) enum RTCStunGatherEventIn {
    SocketWriteFailure(FourTuple),
}

#[derive(Debug)]
pub(crate) enum RTCStunGatherEventOut {
    LocalIceCandidate(RTCIceCandidateInit),
    StunGatheringComplete,
}

/// RTCStunGatherer gathers local host, server reflexive and relay candidates
/// in a Sans-I/O manner.
///
/// This is a Sans-I/O configuration object that holds ICE servers and gathering state.
pub(crate) struct RTCStunGatherer {
    local_addrs: Vec<SocketAddr>,
    ice_servers: Vec<RTCIceServer>,
    ice_gather_policy: RTCIceTransportPolicy,
    state: RTCIceGatheringState,
    /// Host runtime, used to resolve STUN server hostnames.
    runtime: Arc<dyn Runtime>,

    stun_clients: HashMap<FourTuple, StunClient>,

    wouts: VecDeque<TaggedBytesMut>,
    events: VecDeque<RTCStunGatherEventOut>,
}

impl RTCStunGatherer {
    /// Create a new ICE gatherer with ICE servers and gather policy
    pub(crate) fn new(
        local_addrs: Vec<SocketAddr>,
        ice_servers: Vec<RTCIceServer>,
        ice_gather_policy: RTCIceTransportPolicy,
        runtime: Arc<dyn Runtime>,
    ) -> Self {
        Self {
            local_addrs,
            ice_servers,
            ice_gather_policy,
            state: RTCIceGatheringState::New,
            runtime,

            stun_clients: HashMap::new(),

            wouts: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    pub(crate) fn state(&self) -> RTCIceGatheringState {
        self.state
    }

    pub(crate) fn update_configuration(
        &mut self,
        ice_servers: Vec<RTCIceServer>,
        ice_gather_policy: RTCIceTransportPolicy,
    ) {
        for (_, mut stun_client) in self.stun_clients.drain() {
            let _ = stun_client.close();
        }
        self.wouts.clear();
        self.events.clear();
        self.ice_servers = ice_servers;
        self.ice_gather_policy = ice_gather_policy;
        self.state = RTCIceGatheringState::New;
    }

    pub(crate) fn is_stun_message(&self, msg: &TaggedBytesMut) -> bool {
        for four_tuple in self.stun_clients.keys() {
            if four_tuple.peer_addr == msg.transport.peer_addr
                && four_tuple.local_addr == msg.transport.local_addr
            {
                return true;
            }
        }

        false
    }

    pub(crate) async fn gather(&mut self) -> Result<(), Error> {
        self.state = RTCIceGatheringState::Gathering;
        if self.ice_gather_policy != RTCIceTransportPolicy::Relay {
            self.gather_host_candidates()?;
            self.gather_srflx_candidates().await?;
        }
        if self.stun_clients.is_empty() && self.state != RTCIceGatheringState::Complete {
            self.state = RTCIceGatheringState::Complete;
            self.events
                .push_back(RTCStunGatherEventOut::StunGatheringComplete);
        }
        Ok(())
    }

    /// Gather host ICE candidates from a local socket address
    ///
    /// This is a pure function that creates host candidates without performing I/O.
    fn gather_host_candidates(&mut self) -> Result<(), Error> {
        for local_addr in &self.local_addrs {
            let candidate = CandidateHostConfig {
                base_config: CandidateConfig {
                    network: "udp".to_owned(),
                    address: local_addr.ip().to_string(),
                    port: local_addr.port(),
                    component: 1,
                    ..Default::default()
                },
                ..Default::default()
            }
            .new_candidate_host()?;

            let candidate_init = RTCIceCandidate::from(&candidate).to_json()?;

            self.events
                .push_back(RTCStunGatherEventOut::LocalIceCandidate(candidate_init));
        }
        Ok(())
    }

    /// Gather server reflexive (srflx) ICE candidates via STUN
    ///
    /// This performs actual I/O to query STUN servers and should be called
    /// in an async context.
    async fn gather_srflx_candidates(&mut self) -> Result<(), Error> {
        // Clone the handle up front so the per-server borrows below stay disjoint.
        let runtime = Arc::clone(&self.runtime);
        for ice_server in &self.ice_servers {
            for url in &ice_server.urls {
                // Only handle stun: URLs for now
                if !url.starts_with("stun:") {
                    continue;
                }

                // Resolve once per URL, then only create clients for matching families.
                let server_addr = url.strip_prefix("stun:").unwrap_or(url);
                debug!("Resolving STUN server: {}", server_addr);
                let resolved_addrs = match runtime.resolve_host(server_addr).await {
                    Ok(addrs) => addrs,
                    Err(err) => {
                        error!("Failed to resolve STUN server {}: {}", server_addr, err);
                        continue;
                    }
                };

                for local_addr in &self.local_addrs {
                    match RTCStunGatherer::gather_from_stun_server(
                        &*runtime,
                        *local_addr,
                        &resolved_addrs,
                    ) {
                        Ok(Some(stun_client)) => {
                            self.stun_clients.insert(
                                FourTuple {
                                    local_addr: stun_client.local_addr(),
                                    peer_addr: stun_client.peer_addr(),
                                },
                                stun_client,
                            );
                        }
                        Ok(None) => {}
                        Err(err) => {
                            error!("Failed to gather stun client: {}", err);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Gather a single srflx candidate, skipping servers without a matching address family.
    fn gather_from_stun_server(
        runtime: &dyn Runtime,
        local_addr: SocketAddr,
        resolved_addrs: &[SocketAddr],
    ) -> Result<Option<StunClient>, Error> {
        let Some(stun_server_addr) = resolved_addrs
            .iter()
            .copied()
            .find(|addr| addr.is_ipv4() == local_addr.is_ipv4())
        else {
            return Ok(None);
        };

        debug!(
            "STUN server {} selected for {}",
            stun_server_addr, local_addr
        );

        // Create STUN client using the sans-I/O pattern. The client is told the time; it
        // reads no clock of its own, so its transaction deadlines follow this runtime's.
        let now = runtime.now();
        let mut stun_client = StunClientBuilder::new().build(
            now,
            local_addr,
            stun_server_addr,
            TransportProtocol::UDP,
        )?;

        // Create STUN binding request
        let mut msg = StunMessage::new();
        msg.build(&[Box::<TransactionId>::default(), Box::new(BINDING_REQUEST)])?;

        // Send the request
        stun_client.handle_write(TaggedMessage { now, message: msg })?;

        Ok(Some(stun_client))
    }
}

impl Protocol<TaggedBytesMut, (), RTCStunGatherEventIn> for RTCStunGatherer {
    type Rout = ();
    type Wout = TaggedBytesMut;
    type Eout = RTCStunGatherEventOut;
    type Error = Error;
    type Time = Instant;

    fn handle_read(&mut self, msg: TaggedBytesMut) -> Result<(), Self::Error> {
        for (four_tuple, stun_client) in &mut self.stun_clients {
            if four_tuple.peer_addr == msg.transport.peer_addr
                && four_tuple.local_addr == msg.transport.local_addr
            {
                return stun_client.handle_read(msg);
            }
        }

        Ok(())
    }

    fn poll_read(&mut self) -> Option<Self::Rout> {
        None
    }

    fn handle_write(&mut self, _msg: ()) -> Result<(), Self::Error> {
        Ok(())
    }

    fn poll_write(&mut self) -> Option<Self::Wout> {
        for stun_client in self.stun_clients.values_mut() {
            while let Some(transmit) = stun_client.poll_write() {
                self.wouts.push_back(transmit);
            }
        }

        self.wouts.pop_front()
    }

    fn handle_event(&mut self, evt: RTCStunGatherEventIn) -> Result<(), Self::Error> {
        match evt {
            RTCStunGatherEventIn::SocketWriteFailure(four_tuple) => {
                if let Some(mut stun_client) = self.stun_clients.remove(&four_tuple) {
                    let _ = stun_client.close();

                    if self.stun_clients.is_empty() && self.state != RTCIceGatheringState::Complete
                    {
                        self.state = RTCIceGatheringState::Complete;
                        self.events
                            .push_back(RTCStunGatherEventOut::StunGatheringComplete);
                    }
                }
            }
        }
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::Eout> {
        let mut four_tuples = HashSet::new();
        for stun_client in self.stun_clients.values_mut() {
            while let Some(event) = stun_client.poll_event() {
                match event {
                    StunEvent::Message(msg) => {
                        let mut xor_addr = XorMappedAddress::default();
                        if let Err(err) = xor_addr.get_from(&msg) {
                            error!("Failed to get xor mapped message: {}", err);
                            continue;
                        }
                        let config = CandidateServerReflexiveConfig {
                            base_config: CandidateConfig {
                                network: "udp".to_owned(),
                                address: xor_addr.ip.to_string(),
                                port: xor_addr.port,
                                component: 1,
                                ..Default::default()
                            },
                            rel_addr: stun_client.local_addr().ip().to_string(),
                            rel_port: stun_client.local_addr().port(),
                            ..Default::default()
                        };
                        let candidate = match config.new_candidate_server_reflexive() {
                            Ok(candidate) => candidate,
                            Err(err) => {
                                error!("Failed to new_candidate_server_reflexive: {}", err);
                                continue;
                            }
                        };

                        let candidate_init = match RTCIceCandidate::from(&candidate).to_json() {
                            Ok(candidate_init) => candidate_init,
                            Err(err) => {
                                error!("Failed to RTCIceCandidate to json: {}", err);
                                continue;
                            }
                        };

                        four_tuples.insert(FourTuple {
                            local_addr: stun_client.local_addr(),
                            peer_addr: stun_client.peer_addr(),
                        });
                        self.events
                            .push_back(RTCStunGatherEventOut::LocalIceCandidate(candidate_init));
                    }
                    _ => {
                        error!("STUN error: {:?}", event);
                    }
                }
            }
        }

        for four_tuple in four_tuples {
            if let Some(mut stun_client) = self.stun_clients.remove(&four_tuple) {
                let _ = stun_client.close();

                if self.stun_clients.is_empty() && self.state != RTCIceGatheringState::Complete {
                    self.state = RTCIceGatheringState::Complete;
                    self.events
                        .push_back(RTCStunGatherEventOut::StunGatheringComplete);
                }
            }
        }

        self.events.pop_front()
    }

    fn handle_timeout(&mut self, now: Self::Time) -> Result<(), Self::Error> {
        for stun_client in self.stun_clients.values_mut() {
            stun_client.handle_timeout(now)?;
        }
        Ok(())
    }

    fn poll_timeout(&mut self) -> Option<Self::Time> {
        let mut eto: Option<Instant> = None;
        for stun_client in self.stun_clients.values_mut() {
            if let Some(next) = stun_client.poll_timeout() {
                eto = Some(eto.map_or(next, |curr| std::cmp::min(curr, next)));
            }
        }
        eto
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        for (_, mut stun_client) in self.stun_clients.drain() {
            let _ = stun_client.close();
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "runtime-mock"))]
mod tests {
    use super::*;
    use crate::runtime::MockRuntime;

    #[test]
    fn stun_clients_require_a_matching_address_family() {
        let runtime = MockRuntime::new();
        let local_v4: SocketAddr = "192.0.2.1:5000".parse().unwrap();
        let local_v6: SocketAddr = "[2001:db8::1]:5000".parse().unwrap();
        let server_v4: SocketAddr = "192.0.2.2:3478".parse().unwrap();
        let server_v6: SocketAddr = "[2001:db8::2]:3478".parse().unwrap();

        for (local, servers, expected) in [
            (local_v4, vec![server_v6], None),
            (local_v6, vec![server_v4], None),
            (local_v4, vec![], None),
            (local_v6, vec![], None),
            (local_v4, vec![server_v4], Some(server_v4)),
            (local_v6, vec![server_v6], Some(server_v6)),
            (local_v4, vec![server_v6, server_v4], Some(server_v4)),
            (local_v6, vec![server_v4, server_v6], Some(server_v6)),
        ] {
            let client = RTCStunGatherer::gather_from_stun_server(&runtime, local, &servers)
                .expect("an unavailable address family is not an error");
            assert_eq!(client.as_ref().map(|client| client.peer_addr()), expected);
            if let Some(mut client) = client {
                assert_eq!(client.local_addr(), local);
                let request = client.poll_write().expect("binding request queued");
                assert_eq!(request.transport.local_addr, local);
                assert_eq!(Some(request.transport.peer_addr), expected);
            }
        }
    }

    #[test]
    fn single_stack_stun_preserves_dual_stack_host_candidates() {
        for url in ["stun:192.0.2.2:3478", "stun:[2001:db8::2]:3478"] {
            let local_addrs = vec![
                "192.0.2.1:5000".parse().unwrap(),
                "[2001:db8::1]:5000".parse().unwrap(),
            ];
            let mut gatherer = RTCStunGatherer::new(
                local_addrs,
                vec![RTCIceServer {
                    urls: vec![url.to_owned()],
                    ..Default::default()
                }],
                RTCIceTransportPolicy::All,
                Arc::new(MockRuntime::new()),
            );
            futures::executor::block_on(gatherer.gather()).unwrap();
            assert_eq!(gatherer.stun_clients.len(), 1);
            let candidates: Vec<_> = gatherer
                .events
                .iter()
                .filter_map(|event| match event {
                    RTCStunGatherEventOut::LocalIceCandidate(candidate) => {
                        Some(&candidate.candidate)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(candidates.len(), 2);
            assert!(
                candidates
                    .iter()
                    .any(|candidate| candidate.contains("192.0.2.1"))
            );
            assert!(
                candidates
                    .iter()
                    .any(|candidate| candidate.contains("2001:db8::1"))
            );
            let request = gatherer.poll_write().unwrap();
            assert_eq!(
                request.transport.local_addr.is_ipv4(),
                request.transport.peer_addr.is_ipv4()
            );
            assert!(gatherer.poll_write().is_none());
        }
    }

    #[test]
    fn unmatched_server_completes_without_transactions() {
        let mut gatherer = RTCStunGatherer::new(
            vec!["[2001:db8::1]:5000".parse().unwrap()],
            vec![RTCIceServer {
                urls: vec!["stun:192.0.2.2:3478".to_owned()],
                ..Default::default()
            }],
            RTCIceTransportPolicy::All,
            Arc::new(MockRuntime::new()),
        );
        futures::executor::block_on(gatherer.gather()).unwrap();
        assert!(gatherer.stun_clients.is_empty());
        assert!(gatherer.poll_write().is_none());
        assert!(gatherer.poll_timeout().is_none());
        assert_eq!(gatherer.state(), RTCIceGatheringState::Complete);
        assert!(matches!(
            gatherer.poll_event(),
            Some(RTCStunGatherEventOut::LocalIceCandidate(_))
        ));
        assert!(matches!(
            gatherer.poll_event(),
            Some(RTCStunGatherEventOut::StunGatheringComplete)
        ));
        assert!(gatherer.poll_event().is_none());
    }

    #[test]
    fn resolution_failure_does_not_prevent_later_servers() {
        let mut gatherer = RTCStunGatherer::new(
            vec!["192.0.2.1:5000".parse().unwrap()],
            vec![RTCIceServer {
                // MockRuntime only resolves literals, so the first URL fails resolution.
                urls: vec![
                    "stun:unresolvable.invalid:3478".to_owned(),
                    "stun:192.0.2.2:3478".to_owned(),
                ],
                ..Default::default()
            }],
            RTCIceTransportPolicy::All,
            Arc::new(MockRuntime::new()),
        );
        futures::executor::block_on(gatherer.gather()).unwrap();
        assert_eq!(gatherer.stun_clients.len(), 1);
        assert_eq!(
            gatherer.poll_write().unwrap().transport.peer_addr,
            "192.0.2.2:3478".parse().unwrap()
        );
    }
}
