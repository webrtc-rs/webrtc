//! ICE Candidate Gathering (Sans-I/O)
//!
//! This module provides RTCIceGatherer for gathering ICE candidates in a Sans-I/O manner.
//! Unlike the old async version, this gatherer is a configuration object that holds
//! the ICE servers and state.

use crate::{Error, runtime};
use rtc::ice::candidate::CandidateConfig;
use rtc::peer_connection::configuration::{RTCIceServer, RTCIceTransportPolicy};
use rtc::peer_connection::transport::{
    CandidateHostConfig, CandidateServerReflexiveConfig, RTCIceCandidate, RTCIceCandidateInit,
};
use rtc::sansio::Protocol;
use rtc::shared::{FourTuple, TaggedBytesMut, TransportProtocol};
use rtc::stun::{
    client::Client as StunClient, client::ClientBuilder as StunClientBuilder,
    message::BINDING_REQUEST, message::Message as StunMessage, message::TransactionId,
};
/*use rtc::turn::client::{
    Client as TurnClient, ClientConfig as TurnClientConfig, Event as TurnEvent,
};*/
use log::{debug, error};
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::stun::agent::StunEvent;
use rtc::stun::message::Getter;
use rtc::stun::xoraddr::XorMappedAddress;
use std::collections::{HashSet, VecDeque};
use std::net::SocketAddr;
use std::time::Instant;

/// ICEGatherOptions provides options relating to the gathering of ICE candidates.
#[derive(Default, Debug, Clone)]
pub(crate) struct RTCIceGatherOptions {
    pub(crate) ice_servers: Vec<RTCIceServer>,
    pub(crate) ice_gather_policy: RTCIceTransportPolicy,
}

#[derive(Debug)]
pub enum RTCIceGathererEvent {
    LocalIceCandidate(RTCIceCandidateInit),
    IceGatheringComplete,
}

/// RTCIceGatherer gathers local host, server reflexive and relay candidates
/// in a Sans-I/O manner.
///
/// This is a Sans-I/O configuration object that holds ICE servers and gathering state.
pub(crate) struct RTCIceGatherer {
    local_addrs: Vec<SocketAddr>,
    ice_servers: Vec<RTCIceServer>,
    gather_policy: RTCIceTransportPolicy,
    state: RTCIceGatheringState,

    stun_clients: Vec<StunClient>,
    gathering_clients: HashSet<FourTuple>,

    wouts: VecDeque<TaggedBytesMut>,
    events: VecDeque<RTCIceGathererEvent>,
}

impl RTCIceGatherer {
    /// Create a new ICE gatherer with ICE servers and gather policy
    pub(crate) fn new(local_addrs: Vec<SocketAddr>, opts: RTCIceGatherOptions) -> Self {
        Self {
            local_addrs,
            ice_servers: opts.ice_servers,
            gather_policy: opts.ice_gather_policy,
            state: RTCIceGatheringState::New,

            stun_clients: Vec::new(),
            gathering_clients: HashSet::new(),

            wouts: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    pub(crate) fn state(&self) -> RTCIceGatheringState {
        self.state
    }

    pub(crate) fn is_ice_message(&self, msg: &TaggedBytesMut) -> bool {
        for stun_client in &self.stun_clients {
            if stun_client.peer_addr() == msg.transport.peer_addr
                && stun_client.local_addr() == msg.transport.local_addr
            {
                return true;
            }
        }

        false
    }

    pub(crate) async fn gather(&mut self) -> Result<(), Error> {
        self.state = RTCIceGatheringState::Gathering;
        self.gather_host_candidates()?;
        self.gather_srflx_candidates().await?;
        if self.gathering_clients.is_empty() && self.state != RTCIceGatheringState::Complete {
            self.state = RTCIceGatheringState::Complete;
            self.events
                .push_back(RTCIceGathererEvent::IceGatheringComplete);
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
                .push_back(RTCIceGathererEvent::LocalIceCandidate(candidate_init));
        }
        Ok(())
    }

    /// Gather server reflexive (srflx) ICE candidates via STUN
    ///
    /// This performs actual I/O to query STUN servers and should be called
    /// in an async context.
    async fn gather_srflx_candidates(&mut self) -> Result<(), Error> {
        for ice_server in &self.ice_servers {
            for url in &ice_server.urls {
                // Only handle stun: URLs for now
                if !url.starts_with("stun:") {
                    continue;
                }

                for local_addr in &self.local_addrs {
                    match RTCIceGatherer::gather_from_stun_server(*local_addr, url).await {
                        Ok(stun_client) => {
                            self.gathering_clients.insert(FourTuple {
                                local_addr: stun_client.local_addr(),
                                peer_addr: stun_client.peer_addr(),
                            });
                            self.stun_clients.push(stun_client);
                        }
                        Err(err) => {
                            error!("Failed to gather stun client: {}", err);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Gather a single srflx candidate from a STUN server
    async fn gather_from_stun_server(
        local_addr: SocketAddr,
        stun_url: &str,
    ) -> Result<StunClient, Error> {
        // Resolve STUN server address (add default port 3478 if not specified)
        let stun_server_addr_str = if stun_url.contains(':') {
            stun_url
                .strip_prefix("stun:")
                .unwrap_or(stun_url)
                .to_string()
        } else {
            format!(
                "{}:3478",
                stun_url.strip_prefix("stun:").unwrap_or(stun_url)
            )
        };

        debug!("Resolving STUN server: {}", stun_server_addr_str);

        // Resolve hostname to IP address using runtime-agnostic helper
        let resolved_addrs = runtime::resolve_host(&stun_server_addr_str).await?;

        // Filter addresses to match the local_addr IP version (IPv4 or IPv6)
        let stun_server_addr: SocketAddr = resolved_addrs
            .into_iter()
            .find(|addr| addr.is_ipv4() == local_addr.is_ipv4())
            .ok_or_else(|| {
                let ip_version = if local_addr.is_ipv4() { "IPv4" } else { "IPv6" };
                Error::Other(format!(
                    "Failed to resolve STUN server hostname to {} address (local_addr is {})",
                    ip_version, local_addr
                ))
            })?;

        debug!(
            "Resolved STUN server {} to {}",
            stun_server_addr_str, stun_server_addr
        );

        debug!("STUN client bound to {}", local_addr);

        // Create STUN client using the sans-I/O pattern
        let mut stun_client =
            StunClientBuilder::new().build(local_addr, stun_server_addr, TransportProtocol::UDP)?;

        // Create STUN binding request
        let mut msg = StunMessage::new();
        msg.build(&[Box::<TransactionId>::default(), Box::new(BINDING_REQUEST)])?;

        // Send the request
        stun_client.handle_write(msg)?;

        Ok(stun_client)
    }
}

impl Protocol<TaggedBytesMut, (), ()> for RTCIceGatherer {
    type Rout = ();
    type Wout = TaggedBytesMut;
    type Eout = RTCIceGathererEvent;
    type Error = Error;
    type Time = Instant;

    fn handle_read(&mut self, msg: TaggedBytesMut) -> Result<(), Self::Error> {
        for stun_client in &mut self.stun_clients {
            if stun_client.peer_addr() == msg.transport.peer_addr
                && stun_client.local_addr() == msg.transport.local_addr
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
        for stun_client in &mut self.stun_clients {
            while let Some(transmit) = stun_client.poll_write() {
                self.wouts.push_back(transmit);
            }
        }

        self.wouts.pop_front()
    }

    fn handle_event(&mut self, _evt: ()) -> Result<(), Self::Error> {
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::Eout> {
        for stun_client in &mut self.stun_clients {
            let local_addr = stun_client.local_addr();
            let mut peer_addr = None;
            while let Some(event) = stun_client.poll_event() {
                peer_addr = Some(stun_client.peer_addr());
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
                            rel_addr: local_addr.ip().to_string(),
                            rel_port: local_addr.port(),
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
                        self.events
                            .push_back(RTCIceGathererEvent::LocalIceCandidate(candidate_init));
                    }
                    _ => {
                        error!("STUN error: {:?}", event);
                    }
                }
            }
            if let Some(peer_addr) = peer_addr {
                self.gathering_clients.remove(&FourTuple {
                    local_addr,
                    peer_addr,
                });
                if self.gathering_clients.is_empty() && self.state != RTCIceGatheringState::Complete
                {
                    self.state = RTCIceGatheringState::Complete;
                    self.events
                        .push_back(RTCIceGathererEvent::IceGatheringComplete);
                }
            }
        }

        self.events.pop_front()
    }

    fn handle_timeout(&mut self, now: Self::Time) -> Result<(), Self::Error> {
        for stun_client in &mut self.stun_clients {
            stun_client.handle_timeout(now)?;
        }
        Ok(())
    }

    fn poll_timeout(&mut self) -> Option<Self::Time> {
        let mut eto: Option<Instant> = None;
        for stun_client in &mut self.stun_clients {
            if let Some(next) = stun_client.poll_timeout() {
                eto = Some(eto.map_or(next, |curr| std::cmp::min(curr, next)));
            }
        }
        eto
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        for mut stun_client in self.stun_clients.drain(..) {
            stun_client.close()?;
        }
        Ok(())
    }
}
