//! ICE Candidate Gathering (Sans-I/O)
//!
//! This module provides RTCIceGatherer for gathering ICE candidates in a Sans-I/O manner.
//! Unlike the old async version, this gatherer is a configuration object that holds
//! the ICE servers and state.

use crate::runtime;
use rtc::ice::candidate::CandidateConfig;
use rtc::ice::tcp_type::TcpType;
use rtc::peer_connection::configuration::{RTCIceServer, RTCIceTransportPolicy};
use rtc::peer_connection::transport::{
    CandidateHostConfig, CandidateServerReflexiveConfig, RTCIceCandidate, RTCIceCandidateInit,
};
use rtc::sansio::Protocol;
use rtc::shared::error::Error;
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
    /// Addresses of bound TCP passive listeners (emitted as host TCP passive candidates)
    tcp_local_addrs: Vec<SocketAddr>,
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
    pub(crate) fn new(
        local_addrs: Vec<SocketAddr>,
        tcp_local_addrs: Vec<SocketAddr>,
        opts: RTCIceGatherOptions,
    ) -> Self {
        Self {
            local_addrs,
            tcp_local_addrs,
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
        // UDP host candidates
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

        // TCP passive host candidates
        for tcp_addr in &self.tcp_local_addrs {
            let candidate = CandidateHostConfig {
                base_config: CandidateConfig {
                    network: "tcp".to_owned(),
                    address: tcp_addr.ip().to_string(),
                    port: tcp_addr.port(),
                    component: 1,
                    ..Default::default()
                },
                tcp_type: TcpType::Passive,
            }
            .new_candidate_host()?;

            let candidate_init = RTCIceCandidate::from(&candidate).to_json()?;
            self.events
                .push_back(RTCIceGathererEvent::LocalIceCandidate(candidate_init));
        }

        Ok(())
    }

    /// Timeout for DNS resolution of STUN/TURN server hostnames (#774).
    const DNS_RESOLVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

    /// Gather server reflexive (srflx) ICE candidates via STUN
    ///
    /// DNS resolution is performed once per STUN URL (not per local address)
    /// so that an unresolvable hostname incurs at most one timeout rather
    /// than N x timeout for N local addresses.
    async fn gather_srflx_candidates(&mut self) -> Result<(), Error> {
        for ice_server in &self.ice_servers {
            for url in &ice_server.urls {
                // Only handle stun: URLs for now
                if !url.starts_with("stun:") {
                    continue;
                }

                // Resolve STUN hostname once per URL
                let resolved_addrs = match Self::resolve_stun_url(url).await {
                    Ok(addrs) => addrs,
                    Err(err) => {
                        error!("Failed to resolve STUN server {}: {}", url, err);
                        continue;
                    }
                };

                for local_addr in &self.local_addrs {
                    // Pick the address matching the local IP version
                    let stun_server_addr = match resolved_addrs
                        .iter()
                        .find(|addr| addr.is_ipv4() == local_addr.is_ipv4())
                    {
                        Some(addr) => *addr,
                        None => {
                            let ip_ver = if local_addr.is_ipv4() { "IPv4" } else { "IPv6" };
                            debug!(
                                "No {} address for STUN server {} (local_addr {}), skipping",
                                ip_ver, url, local_addr
                            );
                            continue;
                        }
                    };

                    match Self::create_stun_client(*local_addr, stun_server_addr) {
                        Ok(stun_client) => {
                            self.gathering_clients.insert(FourTuple {
                                local_addr: stun_client.local_addr(),
                                peer_addr: stun_client.peer_addr(),
                            });
                            self.stun_clients.push(stun_client);
                        }
                        Err(err) => {
                            error!("Failed to create STUN client: {}", err);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolve a `stun:` URL to a list of socket addresses with a timeout.
    ///
    /// Returns all resolved addresses so the caller can pick the right IP
    /// version per local address without re-resolving.
    async fn resolve_stun_url(stun_url: &str) -> Result<Vec<SocketAddr>, Error> {
        let host_part = stun_url.strip_prefix("stun:").unwrap_or(stun_url);

        // Add default STUN port 3478 when no port is present.
        // `stun:host:port` after stripping the prefix becomes `host:port` which
        // already contains a colon. A bare `stun:hostname` becomes `hostname`
        // with no colon, so we append `:3478`.
        let addr_str = if host_part.contains(':') {
            host_part.to_string()
        } else {
            format!("{}:3478", host_part)
        };

        debug!("Resolving STUN server: {}", addr_str);

        let resolved_addrs =
            runtime::timeout(Self::DNS_RESOLVE_TIMEOUT, runtime::resolve_host(&addr_str))
                .await
                .map_err(|_| {
                    Error::Other(format!(
                        "DNS timed out after {:?} resolving STUN server: {}",
                        Self::DNS_RESOLVE_TIMEOUT,
                        addr_str
                    ))
                })??;

        debug!("Resolved STUN server {} to {:?}", addr_str, resolved_addrs);

        Ok(resolved_addrs)
    }

    /// Create a STUN client for a single (local_addr, stun_server_addr) pair
    /// and enqueue an initial binding request.
    fn create_stun_client(
        local_addr: SocketAddr,
        stun_server_addr: SocketAddr,
    ) -> Result<StunClient, Error> {
        debug!("STUN client bound to {}", local_addr);

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

#[cfg(test)]
mod tests {
    use super::*;

    /// resolve_stun_url with a known-good IP literal should succeed instantly.
    #[test]
    fn test_resolve_stun_url_ip_literal() {
        crate::runtime::block_on(async {
            let addrs = RTCIceGatherer::resolve_stun_url("stun:127.0.0.1:3478")
                .await
                .expect("IP literal should resolve");
            assert!(!addrs.is_empty());
            assert_eq!(addrs[0].ip().to_string(), "127.0.0.1");
            assert_eq!(addrs[0].port(), 3478);
        });
    }

    /// resolve_stun_url with a bare hostname (no port) should append :3478.
    #[test]
    fn test_resolve_stun_url_default_port() {
        crate::runtime::block_on(async {
            let addrs = RTCIceGatherer::resolve_stun_url("stun:127.0.0.1")
                .await
                .expect("bare IP should resolve with default port");
            assert_eq!(addrs[0].port(), 3478);
        });
    }

    /// resolve_stun_url with an unresolvable hostname should return an error
    /// (timeout or DNS failure) rather than hanging.
    #[test]
    fn test_resolve_stun_url_unresolvable() {
        crate::runtime::block_on(async {
            let start = std::time::Instant::now();
            let result =
                RTCIceGatherer::resolve_stun_url("stun:this.will.never.resolve.invalid:3478").await;
            let elapsed = start.elapsed();
            assert!(result.is_err(), "unresolvable hostname should error");
            // Must not hang longer than 2 x DNS_RESOLVE_TIMEOUT
            assert!(
                elapsed.as_secs() < 7,
                "DNS resolution took {:?}, expected < 7s",
                elapsed
            );
        });
    }

    /// create_stun_client should succeed with valid addresses.
    #[test]
    fn test_create_stun_client_valid() {
        let local: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let remote: SocketAddr = "127.0.0.1:3478".parse().unwrap();
        let client = RTCIceGatherer::create_stun_client(local, remote)
            .expect("should create client with valid addrs");
        assert_eq!(client.peer_addr(), remote);
    }

    /// DNS_RESOLVE_TIMEOUT should be 3 seconds.
    #[test]
    fn test_dns_resolve_timeout_value() {
        assert_eq!(
            RTCIceGatherer::DNS_RESOLVE_TIMEOUT,
            std::time::Duration::from_secs(3)
        );
    }
}
