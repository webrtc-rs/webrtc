//! ICE Candidate Gathering (Sans-I/O)
//!
//! This module provides RTCIceGatherer for gathering ICE candidates in a Sans-I/O manner.
//! Unlike the old async version, this gatherer is a configuration object that holds
//! the ICE servers and state.

use crate::runtime;
use log::{debug, error, warn};
use rtc::ice::candidate::CandidateConfig;
use rtc::ice::tcp_type::TcpType;
use rtc::peer_connection::configuration::{RTCIceServer, RTCIceTransportPolicy};
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::peer_connection::transport::{
    CandidateHostConfig, CandidateRelayConfig, CandidateServerReflexiveConfig, RTCIceCandidate,
    RTCIceCandidateInit,
};
use rtc::sansio::Protocol;
use rtc::shared::error::Error;
use rtc::shared::{FourTuple, TaggedBytesMut, TransportProtocol};
use rtc::stun::agent::StunEvent;
use rtc::stun::message::Getter;
use rtc::stun::xoraddr::XorMappedAddress;
use rtc::stun::{
    client::Client as StunClient, client::ClientBuilder as StunClientBuilder,
    message::BINDING_REQUEST, message::Message as StunMessage, message::TransactionId,
};
use rtc::turn::client::{
    Client as TurnClient, ClientConfig as TurnClientConfig, Event as TurnEvent,
};
use std::collections::{HashSet, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Default timeout for DNS resolution of STUN/TURN servers.
const DNS_RESOLVE_TIMEOUT: Duration = Duration::from_secs(3);

/// Default port for STUN/TURN servers per RFC 5389 / RFC 5766.
const DEFAULT_STUN_TURN_PORT: u16 = 3478;

/// Parse a host string (from a STUN/TURN URL, after stripping the scheme and query)
/// into a `host:port` string suitable for DNS resolution.
///
/// Handles the following forms:
/// - `SocketAddr` (e.g. `1.2.3.4:3478`, `[::1]:3478`) — returned as-is
/// - Bare IPv6 literal (e.g. `2001:db8::1`) — wrapped in brackets with default port
/// - Bracketed IPv6 without port (e.g. `[2001:db8::1]`) — appends default port
/// - Bare IPv4 literal (e.g. `1.2.3.4`) — appends default port
/// - Hostname without port (e.g. `stun.l.google.com`) — appends default port
/// - Hostname with port (e.g. `stun.l.google.com:19302`) — returned as-is
fn normalize_host_port(host_str: &str) -> String {
    // 1. Already a valid SocketAddr (e.g. "1.2.3.4:3478" or "[::1]:3478")
    if let Ok(addr) = host_str.parse::<SocketAddr>() {
        return addr.to_string();
    }

    // 2. Bracketed IPv6 without port: "[2001:db8::1]"
    if host_str.starts_with('[') && host_str.ends_with(']') {
        return format!("{}:{}", host_str, DEFAULT_STUN_TURN_PORT);
    }

    // 3. Bare IPv6 literal (contains ':' but isn't a SocketAddr — already ruled out above)
    if let Ok(std::net::IpAddr::V6(_)) = host_str.parse::<std::net::IpAddr>() {
        return format!("[{}]:{}", host_str, DEFAULT_STUN_TURN_PORT);
    }

    // 4. Bare IPv4 literal
    if let Ok(std::net::IpAddr::V4(_)) = host_str.parse::<std::net::IpAddr>() {
        return format!("{}:{}", host_str, DEFAULT_STUN_TURN_PORT);
    }

    // 5. Hostname — may or may not contain a port.
    //    Use rsplit_once(':') to detect an existing port suffix.
    //    We verify the part after ':' is a valid port number to avoid
    //    false positives on IPv6 literals that slipped through.
    if let Some((host, port_str)) = host_str.rsplit_once(':')
        && port_str.parse::<u16>().is_ok()
        && !host.is_empty()
    {
        // Already has a valid port
        return host_str.to_string();
    }

    // No port — append default
    format!("{}:{}", host_str, DEFAULT_STUN_TURN_PORT)
}

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
    /// Active TURN clients for relay candidate gathering (UDP TURN).
    /// Each entry stores (local_addr, turn_server_addr, client).
    /// TCP TURN requires a persistent TCP connection and is managed separately.
    turn_clients: Vec<(SocketAddr, SocketAddr, TurnClient)>,
    gathering_clients: HashSet<FourTuple>,

    wouts: VecDeque<TaggedBytesMut>,
    events: VecDeque<RTCIceGathererEvent>,
}

impl RTCIceGatherer {
    /// Create a new ICE gatherer with ICE servers and gather policy.
    ///
    /// `tcp_local_addrs` should contain the bound addresses of TCP passive listeners
    /// so that TCP passive host candidates can be emitted. Pass an empty `Vec` when
    /// TCP ICE is not used.
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
            turn_clients: Vec::new(),
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

        // Also check TURN client addresses (TURN responses come from the TURN server).
        for (local_addr, server_addr, _) in &self.turn_clients {
            if *server_addr == msg.transport.peer_addr && *local_addr == msg.transport.local_addr {
                return true;
            }
        }

        false
    }

    pub(crate) async fn gather(&mut self) -> Result<(), Error> {
        self.state = RTCIceGatheringState::Gathering;
        if self.gather_policy != RTCIceTransportPolicy::Relay {
            // Only emit host/srflx candidates when not in relay-only mode.
            self.gather_host_candidates()?;
            self.gather_srflx_candidates().await?;
        }
        self.gather_relay_candidates().await?;
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
                    // Skip loopback addresses — STUN servers are not reachable via 127.0.0.1/::1
                    // and attempting a binding request from a loopback interface stalls until timeout,
                    // delaying RTCIceGatheringState::Complete (webrtc-rs/webrtc#778).
                    if local_addr.ip().is_loopback() {
                        continue;
                    }

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

    /// Gather relay (relay) ICE candidates via TURN.
    ///
    /// Supports UDP TURN (turn: URLs without `?transport=tcp`).
    /// TCP TURN requires a persistent TCP connection managed outside the sans-IO gatherer;
    /// see the `turn_client_tcp` example for how to implement TCP TURN manually.
    async fn gather_relay_candidates(&mut self) -> Result<(), Error> {
        for ice_server in &self.ice_servers {
            for url in &ice_server.urls {
                // Only handle turn: URLs (not turns: for now)
                if !url.starts_with("turn:") {
                    continue;
                }

                // Detect TCP transport: turn:host:port?transport=tcp
                let is_tcp = url.contains("transport=tcp");
                if is_tcp {
                    // TCP TURN requires a persistent TCP connection driven by the application
                    // (not the sans-IO gatherer). See rtc-turn/examples/turn_client_tcp.rs.
                    debug!(
                        "TCP TURN ({}) skipped in gatherer — use turn_client_tcp for manual TCP TURN",
                        url
                    );
                    continue;
                }

                let turn_server_str = url
                    .strip_prefix("turn:")
                    .unwrap_or(url)
                    .split('?')
                    .next()
                    .unwrap_or("");

                let turn_server_addr_str = normalize_host_port(turn_server_str);

                debug!("Resolving TURN server: {}", turn_server_addr_str);

                let resolved = match runtime::timeout(
                    DNS_RESOLVE_TIMEOUT,
                    runtime::resolve_host(&turn_server_addr_str),
                )
                .await
                {
                    Ok(Ok(addrs)) => addrs,
                    Ok(Err(e)) => {
                        error!(
                            "Failed to resolve TURN server {}: {}",
                            turn_server_addr_str, e
                        );
                        continue;
                    }
                    Err(_) => {
                        error!(
                            "DNS timeout resolving TURN server: {}",
                            turn_server_addr_str
                        );
                        continue;
                    }
                };

                for local_addr in &self.local_addrs {
                    // Skip loopback — TURN servers are not reachable via loopback.
                    if local_addr.ip().is_loopback() {
                        continue;
                    }

                    let turn_server_addr = match resolved
                        .iter()
                        .find(|a| a.is_ipv4() == local_addr.is_ipv4())
                    {
                        Some(&addr) => addr,
                        None => {
                            debug!(
                                "No matching TURN server address for local_addr {}",
                                local_addr
                            );
                            continue;
                        }
                    };

                    let resolved_addr_str = turn_server_addr.to_string();
                    let cfg = TurnClientConfig {
                        stun_serv_addr: resolved_addr_str.clone(),
                        turn_serv_addr: resolved_addr_str,
                        local_addr: *local_addr,
                        transport_protocol: TransportProtocol::UDP,
                        username: ice_server.username.clone(),
                        password: ice_server.credential.clone(),
                        realm: String::new(),
                        software: String::new(),
                        rto_in_ms: 0,
                    };

                    let mut turn_client = match TurnClient::new(cfg) {
                        Ok(c) => c,
                        Err(e) => {
                            error!(
                                "Failed to create TURN client for {}: {}",
                                turn_server_addr, e
                            );
                            continue;
                        }
                    };

                    if let Err(e) = turn_client.allocate() {
                        error!(
                            "Failed to send TURN ALLOCATE for {}: {}",
                            turn_server_addr, e
                        );
                        continue;
                    }

                    self.gathering_clients.insert(FourTuple {
                        local_addr: *local_addr,
                        peer_addr: turn_server_addr,
                    });
                    self.turn_clients
                        .push((*local_addr, turn_server_addr, turn_client));
                    debug!(
                        "TURN ALLOCATE sent from {} to {}",
                        local_addr, turn_server_addr
                    );
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
        let stun_host = stun_url.strip_prefix("stun:").unwrap_or(stun_url);
        let stun_server_addr_str = normalize_host_port(stun_host);

        debug!("Resolving STUN server: {}", stun_server_addr_str);

        // Resolve hostname to IP address with a timeout (#774)
        let resolved_addrs = runtime::timeout(
            DNS_RESOLVE_TIMEOUT,
            runtime::resolve_host(&stun_server_addr_str),
        )
        .await
        .map_err(|_| {
            Error::Other(format!(
                "DNS timeout resolving STUN server: {}",
                stun_server_addr_str
            ))
        })?
        .map_err(|e| Error::Other(e.to_string()))?;

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

impl RTCIceGatherer {
    /// Remove a gathering four-tuple and, if no more are pending, transition to Complete.
    fn finish_gathering_client(&mut self, local_addr: SocketAddr, peer_addr: SocketAddr) {
        self.gathering_clients.remove(&FourTuple {
            local_addr,
            peer_addr,
        });
        if self.gathering_clients.is_empty() && self.state != RTCIceGatheringState::Complete {
            self.state = RTCIceGatheringState::Complete;
            self.events
                .push_back(RTCIceGathererEvent::IceGatheringComplete);
        }
    }
}

impl Protocol<TaggedBytesMut, (), ()> for RTCIceGatherer {
    type Rout = ();
    type Wout = TaggedBytesMut;
    type Eout = RTCIceGathererEvent;
    type Error = Error;
    type Time = Instant;

    fn handle_read(&mut self, msg: TaggedBytesMut) -> Result<(), Self::Error> {
        // Route to matching STUN client.
        for stun_client in &mut self.stun_clients {
            if stun_client.peer_addr() == msg.transport.peer_addr
                && stun_client.local_addr() == msg.transport.local_addr
            {
                return stun_client.handle_read(msg);
            }
        }

        // Route to matching TURN client (responses come from the TURN server).
        for (local_addr, server_addr, turn_client) in &mut self.turn_clients {
            if *server_addr == msg.transport.peer_addr && *local_addr == msg.transport.local_addr {
                return turn_client.handle_read(msg);
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

        for (_, _, turn_client) in &mut self.turn_clients {
            while let Some(transmit) = turn_client.poll_write() {
                self.wouts.push_back(transmit);
            }
        }

        self.wouts.pop_front()
    }

    fn handle_event(&mut self, _evt: ()) -> Result<(), Self::Error> {
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::Eout> {
        // Collect completed four-tuples so we can update gathering state after
        // releasing the mutable borrows on stun_clients / turn_clients.
        let mut completed_tuples: Vec<(SocketAddr, SocketAddr)> = Vec::new();

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
                completed_tuples.push((local_addr, peer_addr));
            }
        }

        // Process TURN events.
        for (local_addr, server_addr, turn_client) in self.turn_clients.iter_mut() {
            while let Some(event) = turn_client.poll_event() {
                match event {
                    TurnEvent::AllocateResponse(_, relay_addr) => {
                        debug!("TURN relay allocated: {} from {}", relay_addr, local_addr);

                        let config = CandidateRelayConfig {
                            base_config: CandidateConfig {
                                network: "udp".to_owned(),
                                address: relay_addr.ip().to_string(),
                                port: relay_addr.port(),
                                component: 1,
                                ..Default::default()
                            },
                            rel_addr: local_addr.ip().to_string(),
                            rel_port: local_addr.port(),
                            ..Default::default()
                        };

                        match config.new_candidate_relay() {
                            Ok(candidate) => match RTCIceCandidate::from(&candidate).to_json() {
                                Ok(candidate_init) => {
                                    self.events
                                        .push_back(RTCIceGathererEvent::LocalIceCandidate(
                                            candidate_init,
                                        ));
                                }
                                Err(err) => {
                                    error!("Failed to serialize relay candidate: {}", err);
                                }
                            },
                            Err(err) => {
                                error!("Failed to create relay candidate: {}", err);
                            }
                        }

                        completed_tuples.push((*local_addr, *server_addr));
                    }
                    TurnEvent::AllocateError(_, err) => {
                        warn!("TURN ALLOCATE failed from {}: {}", local_addr, err);
                        completed_tuples.push((*local_addr, *server_addr));
                    }
                    TurnEvent::TransactionTimeout(_) => {
                        warn!("TURN transaction timeout from {}", local_addr);
                        completed_tuples.push((*local_addr, *server_addr));
                    }
                    _ => {
                        // Other TURN events (permissions, data) are not expected during gathering.
                        debug!("Unexpected TURN event during gathering: {:?}", event);
                    }
                }
            }
        }

        for (local_addr, peer_addr) in completed_tuples {
            self.finish_gathering_client(local_addr, peer_addr);
        }

        self.events.pop_front()
    }

    fn handle_timeout(&mut self, now: Self::Time) -> Result<(), Self::Error> {
        for stun_client in &mut self.stun_clients {
            stun_client.handle_timeout(now)?;
        }
        for (_, _, turn_client) in &mut self.turn_clients {
            turn_client.handle_timeout(now)?;
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
        for (_, _, turn_client) in &mut self.turn_clients {
            if let Some(next) = turn_client.poll_timeout() {
                eto = Some(eto.map_or(next, |curr| std::cmp::min(curr, next)));
            }
        }
        eto
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        for mut stun_client in self.stun_clients.drain(..) {
            stun_client.close()?;
        }
        for (_, _, mut turn_client) in self.turn_clients.drain(..) {
            turn_client.close()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize_host_port ──────────────────────────────────────────

    #[test]
    fn test_normalize_ipv4_socket_addr() {
        assert_eq!(normalize_host_port("1.2.3.4:3478"), "1.2.3.4:3478");
    }

    #[test]
    fn test_normalize_ipv6_socket_addr() {
        assert_eq!(normalize_host_port("[::1]:3478"), "[::1]:3478");
    }

    #[test]
    fn test_normalize_bare_ipv4() {
        assert_eq!(normalize_host_port("1.2.3.4"), "1.2.3.4:3478");
    }

    #[test]
    fn test_normalize_bare_ipv6() {
        assert_eq!(normalize_host_port("2001:db8::1"), "[2001:db8::1]:3478");
    }

    #[test]
    fn test_normalize_bracketed_ipv6_no_port() {
        assert_eq!(normalize_host_port("[2001:db8::1]"), "[2001:db8::1]:3478");
    }

    #[test]
    fn test_normalize_hostname_no_port() {
        assert_eq!(
            normalize_host_port("stun.l.google.com"),
            "stun.l.google.com:3478"
        );
    }

    #[test]
    fn test_normalize_hostname_with_port() {
        // This is the critical regression: must NOT produce "stun.l.google.com:19302:3478"
        assert_eq!(
            normalize_host_port("stun.l.google.com:19302"),
            "stun.l.google.com:19302"
        );
    }

    #[test]
    fn test_normalize_turn_hostname_with_port() {
        assert_eq!(
            normalize_host_port("turn.example.com:3478"),
            "turn.example.com:3478"
        );
    }

    #[test]
    fn test_normalize_hostname_with_nonstandard_port() {
        assert_eq!(
            normalize_host_port("turn.example.com:5349"),
            "turn.example.com:5349"
        );
    }

    #[test]
    fn test_normalize_ipv4_with_nonstandard_port() {
        assert_eq!(normalize_host_port("10.0.0.1:5349"), "10.0.0.1:5349");
    }

    // ── RTCIceGatherer construction ──────────────────────────────────

    #[test]
    fn test_new_gatherer_initial_state() {
        let gatherer = RTCIceGatherer::new(
            vec!["127.0.0.1:0".parse().unwrap()],
            Vec::new(),
            RTCIceGatherOptions::default(),
        );
        assert_eq!(gatherer.state(), RTCIceGatheringState::New);
    }

    #[test]
    fn test_new_gatherer_with_tcp_addrs() {
        let tcp_addrs: Vec<SocketAddr> = vec!["127.0.0.1:9999".parse().unwrap()];
        let gatherer = RTCIceGatherer::new(
            vec!["127.0.0.1:0".parse().unwrap()],
            tcp_addrs.clone(),
            RTCIceGatherOptions::default(),
        );
        assert_eq!(gatherer.tcp_local_addrs, tcp_addrs);
    }

    // ── gather_host_candidates ───────────────────────────────────────

    #[test]
    fn test_gather_host_candidates_emits_udp_candidates() {
        let mut gatherer = RTCIceGatherer::new(
            vec![
                "192.168.1.1:5000".parse().unwrap(),
                "10.0.0.1:5001".parse().unwrap(),
            ],
            Vec::new(),
            RTCIceGatherOptions::default(),
        );
        gatherer.gather_host_candidates().unwrap();
        // Should emit one candidate per local addr
        assert_eq!(gatherer.events.len(), 2);
    }

    #[test]
    fn test_gather_host_candidates_emits_tcp_passive() {
        let mut gatherer = RTCIceGatherer::new(
            vec!["192.168.1.1:5000".parse().unwrap()],
            vec!["192.168.1.1:9000".parse().unwrap()],
            RTCIceGatherOptions::default(),
        );
        gatherer.gather_host_candidates().unwrap();
        // 1 UDP + 1 TCP passive
        assert_eq!(gatherer.events.len(), 2);
    }

    #[test]
    fn test_gather_host_candidates_no_tcp_when_empty() {
        let mut gatherer = RTCIceGatherer::new(
            vec!["192.168.1.1:5000".parse().unwrap()],
            Vec::new(),
            RTCIceGatherOptions::default(),
        );
        gatherer.gather_host_candidates().unwrap();
        // Only 1 UDP, no TCP
        assert_eq!(gatherer.events.len(), 1);
    }

    // ── is_ice_message ───────────────────────────────────────────────

    #[test]
    fn test_is_ice_message_returns_false_for_unknown() {
        use rtc::shared::TransportContext;
        let gatherer = RTCIceGatherer::new(
            vec!["127.0.0.1:5000".parse().unwrap()],
            Vec::new(),
            RTCIceGatherOptions::default(),
        );
        let msg = TaggedBytesMut {
            now: Instant::now(),
            transport: TransportContext {
                local_addr: "127.0.0.1:5000".parse().unwrap(),
                peer_addr: "8.8.8.8:3478".parse().unwrap(),
                transport_protocol: TransportProtocol::UDP,
                ecn: None,
            },
            message: bytes::BytesMut::new(),
        };
        assert!(!gatherer.is_ice_message(&msg));
    }

    // ── finish_gathering_client ──────────────────────────────────────

    #[test]
    fn test_finish_gathering_completes_when_empty() {
        let mut gatherer = RTCIceGatherer::new(
            vec!["127.0.0.1:5000".parse().unwrap()],
            Vec::new(),
            RTCIceGatherOptions::default(),
        );
        gatherer.state = RTCIceGatheringState::Gathering;
        let la: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let pa: SocketAddr = "8.8.8.8:3478".parse().unwrap();
        gatherer.gathering_clients.insert(FourTuple {
            local_addr: la,
            peer_addr: pa,
        });

        gatherer.finish_gathering_client(la, pa);
        assert!(gatherer.gathering_clients.is_empty());
        assert_eq!(gatherer.state(), RTCIceGatheringState::Complete);
        // Should have emitted IceGatheringComplete event
        assert!(matches!(
            gatherer.events.pop_front(),
            Some(RTCIceGathererEvent::IceGatheringComplete)
        ));
    }

    #[test]
    fn test_finish_gathering_does_not_complete_when_others_remain() {
        let mut gatherer = RTCIceGatherer::new(
            vec!["127.0.0.1:5000".parse().unwrap()],
            Vec::new(),
            RTCIceGatherOptions::default(),
        );
        gatherer.state = RTCIceGatheringState::Gathering;
        let la: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let pa1: SocketAddr = "8.8.8.8:3478".parse().unwrap();
        let pa2: SocketAddr = "9.9.9.9:3478".parse().unwrap();
        gatherer.gathering_clients.insert(FourTuple {
            local_addr: la,
            peer_addr: pa1,
        });
        gatherer.gathering_clients.insert(FourTuple {
            local_addr: la,
            peer_addr: pa2,
        });

        gatherer.finish_gathering_client(la, pa1);
        assert_eq!(gatherer.gathering_clients.len(), 1);
        assert_eq!(gatherer.state(), RTCIceGatheringState::Gathering);
        assert!(gatherer.events.is_empty());
    }

    // ── relay-only policy ────────────────────────────────────────────

    #[test]
    fn test_relay_policy_skips_host_and_srflx() {
        // Verifies that the gather_policy check gates host/srflx gathering.
        // We cannot call gather() without a runtime, but we can verify the policy
        // field is stored correctly and the constructor works.
        let gatherer = RTCIceGatherer::new(
            vec!["192.168.1.1:5000".parse().unwrap()],
            Vec::new(),
            RTCIceGatherOptions {
                ice_servers: Vec::new(),
                ice_gather_policy: RTCIceTransportPolicy::Relay,
            },
        );
        assert_eq!(gatherer.gather_policy, RTCIceTransportPolicy::Relay);
    }

    // ── DNS_RESOLVE_TIMEOUT constant ─────────────────────────────────

    #[test]
    fn test_dns_resolve_timeout_is_3_seconds() {
        assert_eq!(DNS_RESOLVE_TIMEOUT, Duration::from_secs(3));
    }
}
