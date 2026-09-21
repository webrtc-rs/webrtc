//! TURN relayer for async peer connections.

use crate::runtime::Runtime;
use log::{debug, error, trace, warn};
use rtc::crypto::RTCCryptoProvider;
use rtc::ice::url::SchemeType;
use rtc::peer_connection::configuration::{RTCIceServer, RTCIceTransportPolicy};
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::peer_connection::transport::{
    CandidateConfig, CandidateRelayConfig, RTCIceCandidate, RTCIceCandidateInit,
};
use rtc::sansio::Protocol;
use rtc::shared::error::{Error, Result};
use rtc::shared::{FourTuple, TaggedBytesMut, TransportContext, TransportProtocol};
use rtc::stun::message::{
    CLASS_ERROR_RESPONSE, CLASS_INDICATION, CLASS_SUCCESS_RESPONSE, Message as StunMessage,
    TransactionId, is_stun_message,
};
use rtc::turn::client::{
    Client as TurnClient, ClientConfig as TurnClientConfig, Event as TurnEvent,
};
use rtc::turn::proto::chandata::ChannelData;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_PENDING_PACKETS_PER_PEER: usize = 64;

/// Identifies a TCP connect across relayer instances: an ICE restart builds a new relayer,
/// and a connect the old one asked for must not be mistaken for one the new one did.
static NEXT_TCP_CONNECT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A TCP connection the host has been asked to open, and what to do once it is.
#[derive(Debug)]
struct PendingTcpConnect {
    url: String,
    username: String,
    password: String,
}

#[derive(Debug)]
pub(crate) enum RTCTurnRelayEventIn {
    SocketWriteFailure(FourTuple),
}

#[derive(Debug)]
pub(crate) enum RTCTurnRelayEventOut {
    LocalIceCandidate(RTCIceCandidateInit),
    TurnGatheringComplete,
    /// Open a TCP connection to a TURN server and report it with
    /// [`on_tcp_connected`](RTCTurnRelayer::on_tcp_connected) under `id`.
    ConnectTcp {
        id: u64,
        server: SocketAddr,
    },
}

#[derive(Debug)]
struct PendingPermission {
    relay_addr: SocketAddr,
    peer_addr: SocketAddr,
}

struct ManagedTurnClient {
    client: TurnClient,
    /// How this client reaches its server; a packet on the other transport is not its.
    transport: TransportProtocol,
    url: String,
    allocate_tid: rtc::stun::message::TransactionId,
    local_addr: SocketAddr,
    relay_addr: Option<SocketAddr>,
    gather_finished: bool,
}

pub(crate) struct RTCTurnRelayer {
    local_addrs: Vec<SocketAddr>,
    ice_servers: Vec<RTCIceServer>,
    ice_gather_policy: RTCIceTransportPolicy,
    state: RTCIceGatheringState,
    /// Host runtime, used to resolve TURN server hostnames.
    runtime: Arc<dyn Runtime>,
    /// Taken from the peer connection rather than resolved here: the whole connection shares one
    /// provider, and no async-layer code selects crypto on its own.
    crypto_provider: Arc<dyn RTCCryptoProvider>,
    allocation_refresh_interval_cap: Option<Duration>,
    clients: HashMap<FourTuple, ManagedTurnClient>,
    relay_addrs: HashMap<SocketAddr, FourTuple>,
    pending_permissions: HashMap<rtc::stun::message::TransactionId, PendingPermission>,
    pending_permission_pairs: HashMap<(SocketAddr, SocketAddr), rtc::stun::message::TransactionId>,
    pending_packets: HashMap<(SocketAddr, SocketAddr), VecDeque<TaggedBytesMut>>,
    /// TCP connects the host is working on, by id.
    pending_tcp: HashMap<u64, PendingTcpConnect>,
    /// TCP connections whose client is gone, for the host to close.
    released_tcp: Vec<FourTuple>,
    wouts: VecDeque<TaggedBytesMut>,
    routs: VecDeque<TaggedBytesMut>,
    events: VecDeque<RTCTurnRelayEventOut>,
}

impl RTCTurnRelayer {
    pub(crate) fn new(
        local_addrs: Vec<SocketAddr>,
        ice_servers: Vec<RTCIceServer>,
        ice_gather_policy: RTCIceTransportPolicy,
        allocation_refresh_interval_cap: Option<Duration>,
        runtime: Arc<dyn Runtime>,
        crypto_provider: Arc<dyn RTCCryptoProvider>,
    ) -> Self {
        Self {
            local_addrs,
            ice_servers,
            ice_gather_policy,
            state: RTCIceGatheringState::New,
            runtime,
            crypto_provider,
            allocation_refresh_interval_cap,
            clients: HashMap::new(),
            relay_addrs: HashMap::new(),
            pending_permissions: HashMap::new(),
            pending_permission_pairs: HashMap::new(),
            pending_packets: HashMap::new(),
            pending_tcp: HashMap::new(),
            released_tcp: Vec::new(),
            wouts: VecDeque::new(),
            routs: VecDeque::new(),
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
        // A pure credential rotation — same servers, same policy, different username or
        // password — must NOT tear the allocations down. `local_addrs` is fixed at
        // construction (the sockets are shared with host candidates), so a teardown here
        // means the following `gather()` re-`Allocate`s on exactly the same 5-tuple. The
        // server still holds the previous allocation for it and answers **437 (Allocation
        // Mismatch)** per RFC 8656 §7.2, and the restart gathers no relay candidate at all
        // (issue #835).
        //
        // An allocation is a 5-tuple resource, independent of the credential that created
        // it, so the correct move is to re-sign the allocation we already have: update the
        // client's credential and Refresh. Allocation, permissions and channel bindings all
        // survive, so the data path is never interrupted.
        if self.is_credential_only_change(&ice_servers, ice_gather_policy) {
            self.rotate_credentials(&ice_servers);
            self.ice_servers = ice_servers;
            return;
        }

        let keys: Vec<FourTuple> = self.clients.keys().copied().collect();
        for key in keys {
            self.remove_client(key);
        }
        self.relay_addrs.clear();
        self.pending_permissions.clear();
        self.pending_permission_pairs.clear();
        self.pending_packets.clear();
        // A connect still in flight answers to an id nobody holds any more, and its
        // stream is dropped when it arrives.
        self.pending_tcp.clear();
        self.wouts.clear();
        self.routs.clear();
        self.events.clear();
        self.ice_servers = ice_servers;
        self.ice_gather_policy = ice_gather_policy;
        self.state = RTCIceGatheringState::New;
    }

    /// True when `ice_servers` names exactly the same TURN URLs as the running
    /// configuration under the same policy, differing only in credentials.
    ///
    /// URLs are compared after parsing, so formatting differences do not matter. Any change
    /// to the server set, their order, or the transport policy falls through to the full
    /// teardown path — those genuinely need re-gathering, and only the credential-rotation
    /// case can reuse an allocation.
    fn is_credential_only_change(
        &self,
        ice_servers: &[RTCIceServer],
        ice_gather_policy: RTCIceTransportPolicy,
    ) -> bool {
        if ice_gather_policy != self.ice_gather_policy || self.clients.is_empty() {
            return false;
        }

        // Nothing to re-sign unless every live client's allocation is established; a client
        // still mid-Allocate has no allocation to keep. A TCP connect still in flight is one
        // of those too — its client does not exist yet, and would come up on the old password.
        if !self.pending_tcp.is_empty()
            || self
                .clients
                .values()
                .any(|managed| managed.relay_addr.is_none())
        {
            return false;
        }

        let urls_of = |servers: &[RTCIceServer]| -> Option<Vec<String>> {
            let mut out = Vec::new();
            for server in servers {
                for url in server.urls().ok()? {
                    out.push(url.to_string());
                }
            }
            Some(out)
        };

        match (urls_of(ice_servers), urls_of(&self.ice_servers)) {
            (Some(new_urls), Some(old_urls)) => !new_urls.is_empty() && new_urls == old_urls,
            _ => false,
        }
    }

    /// Re-signs every live allocation with the rotated credential and refreshes it.
    ///
    /// Best-effort per client: a client whose URL is no longer present, or whose refresh
    /// fails to encode, is left alone rather than torn down — its allocation keeps working
    /// on the old credential until it expires, which is strictly better than forcing the
    /// 437 this path exists to avoid.
    fn rotate_credentials(&mut self, ice_servers: &[RTCIceServer]) {
        // url string -> (username, credential), matching how `gather` derives them.
        let mut credentials: HashMap<String, (String, String)> = HashMap::new();
        for ice_server in ice_servers {
            let Ok(urls) = ice_server.urls() else {
                continue;
            };
            for url in urls {
                credentials.insert(
                    url.to_string(),
                    (url.username.clone(), url.password.clone()),
                );
            }
        }

        for managed in self.clients.values_mut() {
            let Some((username, password)) = credentials.get(&managed.url) else {
                continue;
            };

            let _ = managed
                .client
                .update_credentials(username.clone(), password.clone());

            if let Err(err) = managed.client.refresh_allocations(self.runtime.now()) {
                warn!(
                    "TURN credential rotation: refresh failed for {} via {}: {}",
                    managed.local_addr, managed.url, err
                );
            } else {
                debug!(
                    "TURN credentials rotated for {} via {}, allocation kept",
                    managed.local_addr, managed.url
                );
            }
        }
    }

    /// The outcome of a [`ConnectTcp`](RTCTurnRelayEventOut::ConnectTcp). Returns the
    /// four-tuple to register as a TURN stream, or `None` when there is nothing to keep.
    pub(crate) fn on_tcp_connected(
        &mut self,
        id: u64,
        result: std::io::Result<FourTuple>,
    ) -> Result<Option<FourTuple>> {
        let Some(pending) = self.pending_tcp.remove(&id) else {
            // Asked for before a restart or a reconfiguration: not ours any more.
            return Ok(None);
        };
        let four_tuple = match result {
            Ok(four_tuple) => four_tuple,
            Err(err) => {
                warn!(
                    "TCP connection to TURN server {} failed: {}",
                    pending.url, err
                );
                self.maybe_emit_gathering_complete();
                return Ok(None);
            }
        };
        if self.clients.contains_key(&four_tuple) {
            warn!(
                "TCP connection {:?} to {} collides with an existing TURN client",
                four_tuple, pending.url
            );
            self.maybe_emit_gathering_complete();
            return Ok(None);
        }

        let mut client = TurnClient::new(
            TurnClientConfig {
                stun_serv_addr: four_tuple.peer_addr.to_string(),
                turn_serv_addr: four_tuple.peer_addr.to_string(),
                local_addr: four_tuple.local_addr,
                transport_protocol: TransportProtocol::TCP,
                // The connection is TCP; the relay is still UDP (RFC 8656 §7.1).
                requested_transport: TransportProtocol::UDP,
                username: pending.username,
                password: pending.password,
                realm: String::new(),
                software: String::new(),
                rto_in_ms: 0,
                allocation_refresh_interval_cap: self.allocation_refresh_interval_cap,
            },
            Arc::clone(&self.crypto_provider),
        )?;
        let allocate_tid = client.allocate(self.runtime.now())?;
        debug!(
            "TURN allocation started over TCP from {} to {} via {}",
            four_tuple.local_addr, four_tuple.peer_addr, pending.url
        );
        self.clients.insert(
            four_tuple,
            ManagedTurnClient {
                client,
                transport: TransportProtocol::TCP,
                url: pending.url,
                allocate_tid,
                local_addr: four_tuple.local_addr,
                relay_addr: None,
                gather_finished: false,
            },
        );
        Ok(Some(four_tuple))
    }

    /// TCP connections whose client this relayer has let go of, for the host to close.
    pub(crate) fn take_released_tcp_streams(&mut self) -> Vec<FourTuple> {
        std::mem::take(&mut self.released_tcp)
    }

    pub(crate) fn is_turn_message(&self, msg: &TaggedBytesMut) -> bool {
        self.matching_client_key(msg).is_some()
    }

    pub(crate) fn contains_local_addr(&self, local_addr: SocketAddr) -> bool {
        self.relay_addrs.contains_key(&local_addr)
    }

    pub(crate) async fn gather(&mut self) -> Result<()> {
        if self.state == RTCIceGatheringState::Gathering {
            return Ok(());
        }

        if self.state == RTCIceGatheringState::Complete {
            self.emit_existing_candidates()?;
            self.events
                .push_back(RTCTurnRelayEventOut::TurnGatheringComplete);
            return Ok(());
        }

        self.state = RTCIceGatheringState::Gathering;

        // Clone the handle up front so the per-server borrows below stay disjoint.
        let runtime = Arc::clone(&self.runtime);

        for ice_server in &self.ice_servers {
            let urls = ice_server.urls()?;

            for url in urls {
                if !matches!(url.scheme, SchemeType::Turn | SchemeType::Turns) {
                    continue;
                }

                if url.is_secure() {
                    warn!("Skipping unsupported secure TURN url {}", url);
                    continue;
                }

                let over_tcp = match url.proto.to_string().as_str() {
                    "udp" => false,
                    "tcp" => true,
                    other => {
                        warn!(
                            "Skipping TURN url {} with unsupported transport {}",
                            url, other
                        );
                        continue;
                    }
                };

                let turn_server_addr = format!("{}:{}", url.host, url.port);
                let resolved_addrs = match runtime.resolve_host(&turn_server_addr).await {
                    Ok(addrs) => addrs,
                    Err(err) => {
                        error!(
                            "Failed to resolve TURN server {}: {}",
                            turn_server_addr, err
                        );
                        continue;
                    }
                };

                if over_tcp {
                    // One connection per server and address family: its source address is
                    // the kernel's choice and has nothing to do with the UDP sockets.
                    let mut families: Vec<bool> =
                        self.local_addrs.iter().map(SocketAddr::is_ipv4).collect();
                    families.sort_unstable();
                    families.dedup();
                    for is_ipv4 in families {
                        let Some(server) = resolved_addrs
                            .iter()
                            .copied()
                            .find(|addr| addr.is_ipv4() == is_ipv4)
                        else {
                            continue;
                        };
                        let id =
                            NEXT_TCP_CONNECT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        self.pending_tcp.insert(
                            id,
                            PendingTcpConnect {
                                url: url.to_string(),
                                username: url.username.clone(),
                                password: url.password.clone(),
                            },
                        );
                        self.events
                            .push_back(RTCTurnRelayEventOut::ConnectTcp { id, server });
                    }
                    continue;
                }

                for local_addr in &self.local_addrs {
                    let Some(peer_addr) = resolved_addrs
                        .iter()
                        .copied()
                        .find(|addr| addr.is_ipv4() == local_addr.is_ipv4())
                    else {
                        continue;
                    };

                    let four_tuple = FourTuple {
                        local_addr: *local_addr,
                        peer_addr,
                    };
                    if self.clients.contains_key(&four_tuple) {
                        continue;
                    }

                    let mut client = TurnClient::new(
                        TurnClientConfig {
                            stun_serv_addr: peer_addr.to_string(),
                            turn_serv_addr: peer_addr.to_string(),
                            local_addr: *local_addr,
                            transport_protocol: TransportProtocol::UDP,
                            requested_transport: TransportProtocol::UDP,
                            username: url.username.clone(),
                            password: url.password.clone(),
                            realm: String::new(),
                            software: String::new(),
                            rto_in_ms: 0,
                            allocation_refresh_interval_cap: self.allocation_refresh_interval_cap,
                        },
                        Arc::clone(&self.crypto_provider),
                    )?;

                    let allocate_tid = client.allocate(self.runtime.now())?;
                    debug!(
                        "TURN allocation started from {} to {} via {}",
                        local_addr, peer_addr, url
                    );

                    self.clients.insert(
                        four_tuple,
                        ManagedTurnClient {
                            client,
                            transport: TransportProtocol::UDP,
                            url: url.to_string(),
                            allocate_tid,
                            local_addr: *local_addr,
                            relay_addr: None,
                            gather_finished: false,
                        },
                    );
                }
            }
        }

        if self.clients.is_empty() && self.pending_tcp.is_empty() {
            self.state = RTCIceGatheringState::Complete;
            self.events
                .push_back(RTCTurnRelayEventOut::TurnGatheringComplete);
        }

        Ok(())
    }

    fn emit_existing_candidates(&mut self) -> Result<()> {
        for managed_client in self.clients.values() {
            if let Some(relay_addr) = managed_client.relay_addr {
                self.events
                    .push_back(RTCTurnRelayEventOut::LocalIceCandidate(
                        Self::build_local_candidate(
                            relay_addr,
                            managed_client.local_addr,
                            &managed_client.url,
                        )?,
                    ));
            }
        }

        Ok(())
    }

    fn build_local_candidate(
        relay_addr: SocketAddr,
        local_addr: SocketAddr,
        url: &str,
    ) -> Result<RTCIceCandidateInit> {
        let candidate = CandidateRelayConfig {
            base_config: CandidateConfig {
                network: "udp".to_owned(),
                address: relay_addr.ip().to_string(),
                port: relay_addr.port(),
                component: 1,
                ..Default::default()
            },
            rel_addr: local_addr.ip().to_string(),
            rel_port: local_addr.port(),
            url: Some(url.to_owned()),
        }
        .new_candidate_relay()?;

        let mut candidate_init = RTCIceCandidate::from(&candidate).to_json()?;
        candidate_init.url = Some(url.to_owned());
        Ok(candidate_init)
    }

    fn maybe_emit_gathering_complete(&mut self) {
        if self.state == RTCIceGatheringState::Gathering
            && self.pending_tcp.is_empty()
            && self.clients.values().all(|client| client.gather_finished)
        {
            self.state = RTCIceGatheringState::Complete;
            self.events
                .push_back(RTCTurnRelayEventOut::TurnGatheringComplete);
        }
    }

    /// Which TURN client, if any, this inbound packet belongs to.
    ///
    /// These sockets are shared with [`RTCStunGatherer`](super::stun_gatherer::RTCStunGatherer),
    /// and when the configuration points `stun:` and `turn:` URLs at one `host:port` — the standard
    /// single-listener coturn deployment — the four-tuple is *identical* for both users. So the
    /// message is inspected before the four-tuple is trusted, because in that deployment an address
    /// match cannot say whose the packet is: a response is matched by transaction id, and only the
    /// framings that carry none fall back to addresses. See [webrtc#890].
    ///
    /// [webrtc#890]: https://github.com/webrtc-rs/webrtc/issues/890
    fn matching_client_key(&self, msg: &TaggedBytesMut) -> Option<FourTuple> {
        // ChannelData is not STUN-framed and carries no transaction id, so addresses are all there
        // is — and they suffice, because the gatherer neither sends nor receives it.
        if ChannelData::is_channel_data(&msg.message) {
            return self.match_by_local_addr(msg);
        }

        if !is_stun_message(&msg.message) {
            return None;
        }

        let mut stun_message = StunMessage::new();
        stun_message.raw = msg.message.to_vec();
        if stun_message.decode().is_err() {
            return None;
        }

        match stun_message.typ.class {
            // Data and Send indications are not transaction-matched, so they route by address for
            // the same reason ChannelData does.
            CLASS_INDICATION => self.match_by_local_addr(msg),

            // RFC 5389 section 7.3.3: a response belongs to whoever sent the request, identified by
            // transaction id and by nothing else. Every TURN request registers one, so a response
            // no client is waiting on is not ours — most often a Binding response for the gatherer,
            // arriving from the address we share with it.
            CLASS_SUCCESS_RESPONSE | CLASS_ERROR_RESPONSE => self.match_by_transaction(
                &stun_message.transaction_id,
                msg.transport.transport_protocol,
            ),

            // An inbound request on these sockets is an ICE connectivity check, which belongs to
            // neither this relayer nor the gatherer.
            _ => None,
        }
    }

    /// The client waiting on `transaction_id`.
    ///
    /// Searches every client rather than the one whose four-tuple matches, because a multi-homed
    /// server may answer from an address other than the one we sent to — the case
    /// [`match_same_local_client`](Self::match_same_local_client) exists for. A transaction id
    /// identifies the client outright, so that heuristic is not needed here.
    fn match_by_transaction(
        &self,
        transaction_id: &TransactionId,
        transport: TransportProtocol,
    ) -> Option<FourTuple> {
        self.clients
            .iter()
            .find(|(_, managed)| {
                managed.transport == transport && managed.client.has_transaction(transaction_id)
            })
            .map(|(four_tuple, _)| *four_tuple)
    }

    /// The client on this packet's local socket, disambiguated by peer address.
    ///
    /// For the framings that carry no transaction id. Prefers an exact four-tuple, then falls back
    /// to clients sharing the local socket.
    fn match_by_local_addr(&self, msg: &TaggedBytesMut) -> Option<FourTuple> {
        // A datagram never belongs to a stream's client, nor the other way round, even when
        // a TCP source port happens to equal a UDP socket's.
        let transport = msg.transport.transport_protocol;
        let exact = FourTuple::from(&msg.transport);
        if self
            .clients
            .get(&exact)
            .is_some_and(|managed| managed.transport == transport)
        {
            return Some(exact);
        }

        let same_local: Vec<FourTuple> = self
            .clients
            .iter()
            .filter(|(four_tuple, managed)| {
                managed.transport == transport && four_tuple.local_addr == msg.transport.local_addr
            })
            .map(|(four_tuple, _)| *four_tuple)
            .collect();

        Self::match_same_local_client(&same_local, msg.transport.peer_addr)
    }

    fn match_same_local_client(
        candidates: &[FourTuple],
        peer_addr: SocketAddr,
    ) -> Option<FourTuple> {
        if candidates.len() == 1 {
            return Some(candidates[0]);
        }

        if let Some(exact) = candidates
            .iter()
            .copied()
            .find(|four_tuple| four_tuple.peer_addr == peer_addr)
        {
            return Some(exact);
        }

        let mut matching_port = candidates
            .iter()
            .copied()
            .filter(|four_tuple| four_tuple.peer_addr.port() == peer_addr.port());
        let first = matching_port.next()?;
        if matching_port.next().is_none() {
            Some(first)
        } else {
            None
        }
    }

    fn remove_client(&mut self, four_tuple: FourTuple) {
        if let Some(mut managed_client) = self.clients.remove(&four_tuple) {
            if let Some(relay_addr) = managed_client.relay_addr.take() {
                self.relay_addrs.remove(&relay_addr);
                self.pending_packets
                    .retain(|(addr, _), _| *addr != relay_addr);
                self.pending_permissions
                    .retain(|_, pending| pending.relay_addr != relay_addr);
                self.pending_permission_pairs
                    .retain(|(addr, _), _| *addr != relay_addr);
            }
            let _ = managed_client.client.close();
            if managed_client.transport == TransportProtocol::TCP {
                self.released_tcp.push(four_tuple);
            }
        }
    }

    fn buffer_packet(
        &mut self,
        relay_addr: SocketAddr,
        peer_addr: SocketAddr,
        packet: TaggedBytesMut,
    ) {
        let queue = self
            .pending_packets
            .entry((relay_addr, peer_addr))
            .or_default();
        if queue.len() >= MAX_PENDING_PACKETS_PER_PEER {
            let _ = queue.pop_front();
        }
        queue.push_back(packet);
    }

    fn flush_pending_packets(&mut self, relay_addr: SocketAddr, peer_addr: SocketAddr) {
        let Some(four_tuple) = self.relay_addrs.get(&relay_addr).copied() else {
            return;
        };
        let Some(mut packets) = self.pending_packets.remove(&(relay_addr, peer_addr)) else {
            return;
        };
        let Some(managed_client) = self.clients.get_mut(&four_tuple) else {
            return;
        };

        while let Some(packet) = packets.pop_front() {
            match managed_client
                .client
                .relay(relay_addr)
                .and_then(|mut relay| relay.send_to(packet.now, &packet.message, peer_addr))
            {
                Ok(()) => {}
                Err(Error::ErrNoPermission) => {
                    self.pending_packets
                        .entry((relay_addr, peer_addr))
                        .or_default()
                        .push_front(packet);
                    break;
                }
                Err(err) => {
                    error!(
                        "Failed to flush buffered relay packet to {} via {}: {}",
                        peer_addr, relay_addr, err
                    );
                }
            }
        }
    }
}

impl Protocol<TaggedBytesMut, TaggedBytesMut, RTCTurnRelayEventIn> for RTCTurnRelayer {
    type Rout = TaggedBytesMut;
    type Wout = TaggedBytesMut;
    type Eout = RTCTurnRelayEventOut;
    type Error = Error;
    type Time = Instant;

    fn handle_read(&mut self, msg: TaggedBytesMut) -> Result<()> {
        if let Some(client_key) = self.matching_client_key(&msg)
            && let Some(managed_client) = self.clients.get_mut(&client_key)
        {
            managed_client.client.handle_read(msg)?;
        }
        Ok(())
    }

    fn poll_read(&mut self) -> Option<Self::Rout> {
        self.routs.pop_front()
    }

    fn handle_write(&mut self, msg: TaggedBytesMut) -> Result<()> {
        let relay_addr = msg.transport.local_addr;
        let peer_addr = msg.transport.peer_addr;

        let Some(four_tuple) = self.relay_addrs.get(&relay_addr).copied() else {
            return Err(Error::Other(format!(
                "unknown relay local address {} for outbound packet",
                relay_addr
            )));
        };
        let Some(managed_client) = self.clients.get_mut(&four_tuple) else {
            return Err(Error::Other(format!(
                "missing TURN client for relay local address {}",
                relay_addr
            )));
        };

        match managed_client
            .client
            .relay(relay_addr)
            .and_then(|mut relay| relay.send_to(msg.now, &msg.message, peer_addr))
        {
            Ok(()) => Ok(()),
            Err(Error::ErrNoPermission) => {
                if !self
                    .pending_permission_pairs
                    .contains_key(&(relay_addr, peer_addr))
                    && let Some(tid) = managed_client
                        .client
                        .relay(relay_addr)?
                        .create_permission(msg.now, peer_addr)?
                {
                    self.pending_permissions.insert(
                        tid,
                        PendingPermission {
                            relay_addr,
                            peer_addr,
                        },
                    );
                    self.pending_permission_pairs
                        .insert((relay_addr, peer_addr), tid);
                }

                self.buffer_packet(relay_addr, peer_addr, msg);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn poll_write(&mut self) -> Option<Self::Wout> {
        for managed_client in self.clients.values_mut() {
            while let Some(msg) = managed_client.client.poll_write() {
                self.wouts.push_back(msg);
            }
        }
        self.wouts.pop_front()
    }

    fn handle_event(&mut self, evt: RTCTurnRelayEventIn) -> Result<()> {
        match evt {
            RTCTurnRelayEventIn::SocketWriteFailure(four_tuple) => {
                self.remove_client(four_tuple);
                self.maybe_emit_gathering_complete();
            }
        }
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::Eout> {
        let keys: Vec<FourTuple> = self.clients.keys().copied().collect();
        for four_tuple in keys {
            let mut gathered_complete = false;
            let mut local_candidate = None;
            let mut pending_flush: Vec<(SocketAddr, SocketAddr)> = vec![];
            let mut pending_drop: Vec<(SocketAddr, SocketAddr)> = vec![];
            let mut read_msgs: Vec<TaggedBytesMut> = vec![];

            if let Some(managed_client) = self.clients.get_mut(&four_tuple) {
                while let Some(event) = managed_client.client.poll_event() {
                    match event {
                        TurnEvent::AllocateResponse(tid, relay_addr)
                            if tid == managed_client.allocate_tid =>
                        {
                            managed_client.relay_addr = Some(relay_addr);
                            managed_client.gather_finished = true;
                            self.relay_addrs.insert(relay_addr, four_tuple);
                            local_candidate = Some(Self::build_local_candidate(
                                relay_addr,
                                managed_client.local_addr,
                                &managed_client.url,
                            ));
                            gathered_complete = true;
                        }
                        TurnEvent::AllocateError(tid, err)
                            if tid == managed_client.allocate_tid =>
                        {
                            error!(
                                "TURN allocation failed from {} to {}: {}",
                                four_tuple.local_addr, four_tuple.peer_addr, err
                            );
                            managed_client.gather_finished = true;
                            gathered_complete = true;
                        }
                        TurnEvent::CreatePermissionResponse(tid, peer_addr) => {
                            if let Some(pending) = self.pending_permissions.remove(&tid) {
                                self.pending_permission_pairs
                                    .remove(&(pending.relay_addr, pending.peer_addr));
                                pending_flush.push((pending.relay_addr, peer_addr));
                            }
                        }
                        TurnEvent::CreatePermissionError(tid, err) => {
                            error!("TURN permission request failed: {}", err);
                            if let Some(pending) = self.pending_permissions.remove(&tid) {
                                self.pending_permission_pairs
                                    .remove(&(pending.relay_addr, pending.peer_addr));
                                pending_drop.push((pending.relay_addr, pending.peer_addr));
                            }
                        }
                        TurnEvent::DataIndicationOrChannelData(_, peer_addr, data) => {
                            if let Some(relay_addr) = managed_client.relay_addr {
                                read_msgs.push(TaggedBytesMut {
                                    now: self.runtime.now(),
                                    transport: TransportContext {
                                        local_addr: relay_addr,
                                        peer_addr,
                                        ecn: None,
                                        transport_protocol: TransportProtocol::UDP,
                                    },
                                    message: data,
                                });
                            }
                        }
                        TurnEvent::TransactionTimeout(tid) => {
                            error!("TURN transaction timed out: {:?}", tid);
                            if let Some(pending) = self.pending_permissions.remove(&tid) {
                                self.pending_permission_pairs
                                    .remove(&(pending.relay_addr, pending.peer_addr));
                                pending_drop.push((pending.relay_addr, pending.peer_addr));
                            } else if tid == managed_client.allocate_tid {
                                managed_client.gather_finished = true;
                                gathered_complete = true;
                            }
                        }
                        TurnEvent::BindingResponse(_, _) | TurnEvent::BindingError(_, _) => {}
                        _ => {
                            warn!("Ignoring unknown TurnEvent variant");
                        }
                    }
                }
            }

            for (relay_addr, peer_addr) in pending_flush {
                self.flush_pending_packets(relay_addr, peer_addr);
            }
            for (relay_addr, peer_addr) in pending_drop {
                self.pending_packets.remove(&(relay_addr, peer_addr));
            }
            for msg in read_msgs {
                self.routs.push_back(msg);
            }
            if let Some(candidate_result) = local_candidate {
                match candidate_result {
                    Ok(candidate) => {
                        trace!("LocalRelayCandidate {:?}", candidate);
                        self.events
                            .push_back(RTCTurnRelayEventOut::LocalIceCandidate(candidate));
                    }
                    Err(err) => {
                        error!("failed to build relay candidate after allocation: {}", err);
                    }
                }
            }
            if gathered_complete {
                self.maybe_emit_gathering_complete();
            }
        }

        self.events.pop_front()
    }

    fn handle_timeout(&mut self, now: Self::Time) -> Result<()> {
        for managed_client in self.clients.values_mut() {
            managed_client.client.handle_timeout(now)?;
        }
        Ok(())
    }

    fn poll_timeout(&mut self) -> Option<Self::Time> {
        let mut eto = None;
        for managed_client in self.clients.values_mut() {
            if let Some(next) = managed_client.client.poll_timeout() {
                eto = Some(eto.map_or(next, |current| std::cmp::min(current, next)));
            }
        }
        eto
    }

    fn close(&mut self) -> Result<()> {
        let keys: Vec<FourTuple> = self.clients.keys().copied().collect();
        for key in keys {
            self.remove_client(key);
        }
        Ok(())
    }
}

// A built-in provider is required: these construct real peer connections, and construction
// resolves a provider. The no-built-in configuration is exercised by the provider tests in
// `tests/`, which supply their own.
#[cfg(all(test, any(feature = "crypto-ring", feature = "crypto-aws-lc-rs")))]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use rtc::peer_connection::configuration::RTCIceServer;
    use rtc::stun::attributes::{ATTR_NONCE, ATTR_REALM};
    use rtc::stun::error_code::CODE_UNAUTHORIZED;
    use rtc::stun::message::{
        CLASS_ERROR_RESPONSE, CLASS_SUCCESS_RESPONSE, MessageType, TransactionId,
    };
    use rtc::stun::textattrs::{Nonce, Realm};
    use rtc::stun::xoraddr::XorMappedAddress;
    use std::net::{IpAddr, Ipv4Addr};

    fn build_binding_success(transaction_id: TransactionId) -> StunMessage {
        let mut msg = StunMessage::new();
        msg.build(&[
            Box::new(transaction_id),
            Box::new(MessageType::new(
                rtc::stun::message::METHOD_BINDING,
                CLASS_SUCCESS_RESPONSE,
            )),
            Box::new(XorMappedAddress {
                ip: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7)),
                port: 51234,
            }),
        ])
        .expect("failed to build Binding success response");
        msg
    }

    fn tagged(local_addr: SocketAddr, peer_addr: SocketAddr, raw: &[u8]) -> TaggedBytesMut {
        TaggedBytesMut {
            now: Instant::now(), // Exemption: usage in #test code
            transport: TransportContext {
                local_addr,
                peer_addr,
                ecn: None,
                transport_protocol: TransportProtocol::UDP,
            },
            message: BytesMut::from(raw),
        }
    }

    fn build_turn_allocate_unauthorized(transaction_id: TransactionId) -> StunMessage {
        let mut msg = StunMessage::new();
        msg.build(&[
            Box::new(transaction_id),
            Box::new(MessageType::new(
                rtc::stun::message::METHOD_ALLOCATE,
                CLASS_ERROR_RESPONSE,
            )),
            Box::new(CODE_UNAUTHORIZED),
            Box::new(Realm::new(ATTR_REALM, "webrtc.rs".to_owned())),
            Box::new(Nonce::new(ATTR_NONCE, "nonce".to_owned())),
        ])
        .expect("failed to build TURN unauthorized response");
        msg
    }

    #[test]
    fn routes_turn_allocate_response_by_local_addr_and_port() {
        futures::executor::block_on(async {
            let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
            let turn_peer_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3478);
            let mut relayer = RTCTurnRelayer::new(
                vec![local_addr],
                vec![RTCIceServer {
                    urls: vec![format!("turn:{}?transport=udp", turn_peer_addr)],
                    username: "user".to_owned(),
                    credential: "pass".to_owned(),
                }],
                RTCIceTransportPolicy::Relay,
                None,
                crate::runtime::default_runtime().expect("test requires a runtime feature"),
                rtc::crypto::default_provider().expect("a built-in crypto provider for tests"),
            );

            relayer.gather().await.expect("TURN gather should start");
            let initial_request = relayer.poll_write().expect("initial Allocate request");
            assert_eq!(initial_request.transport.peer_addr, turn_peer_addr);

            let mut initial_request_msg = StunMessage::new();
            initial_request_msg.raw = initial_request.message.to_vec();
            initial_request_msg
                .decode()
                .expect("decode initial Allocate request");

            let response = build_turn_allocate_unauthorized(initial_request_msg.transaction_id);
            let msg = TaggedBytesMut {
                now: Instant::now(), // Exemption: usage in #test code
                transport: TransportContext {
                    local_addr,
                    peer_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 3478),
                    ecn: None,
                    transport_protocol: TransportProtocol::UDP,
                },
                message: BytesMut::from(&response.raw[..]),
            };

            assert!(
                relayer.is_turn_message(&msg),
                "TURN error response on the same local socket and TURN port should route to the relayer"
            );

            relayer
                .handle_read(msg)
                .expect("relayer should accept TURN unauthorized response");

            let retry_request = relayer
                .poll_write()
                .expect("authenticated Allocate retry after unauthorized response");
            assert_eq!(retry_request.transport.peer_addr.port(), 3478);
            assert!(
                retry_request.message.len() > initial_request.message.len(),
                "authenticated retry should be larger than the unauthenticated Allocate request"
            );
        });
    }

    /// webrtc#890: with `stun:` and `turn:` on one address, the Binding response the gatherer is
    /// waiting for arrives on the TURN client's exact four-tuple. Claimed here, it is discarded for
    /// having no matching transaction and no server-reflexive candidate is ever gathered.
    #[test]
    fn binding_response_from_turn_server_addr_is_not_turn_traffic() {
        futures::executor::block_on(async {
            let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
            let shared_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3478);
            let mut relayer = RTCTurnRelayer::new(
                vec![local_addr],
                vec![RTCIceServer {
                    urls: vec![format!("turn:{}?transport=udp", shared_addr)],
                    username: "user".to_owned(),
                    credential: "pass".to_owned(),
                }],
                RTCIceTransportPolicy::Relay,
                None,
                crate::runtime::default_runtime().expect("test requires a runtime feature"),
                rtc::crypto::default_provider().expect("a built-in crypto provider for tests"),
            );

            relayer.gather().await.expect("TURN gather should start");
            relayer.poll_write().expect("initial Allocate request");

            // The gatherer's transaction, not the relayer's, on the shared four-tuple.
            let response = build_binding_success(TransactionId::new());
            let msg = tagged(local_addr, shared_addr, &response.raw);

            assert!(
                !relayer.is_turn_message(&msg),
                "a Binding response is the STUN gatherer's even on the TURN client's four-tuple"
            );
        });
    }

    /// The guard must not cost the relayer its own traffic on the same four-tuple.
    #[test]
    fn allocate_response_on_exact_four_tuple_still_routes_to_turn() {
        futures::executor::block_on(async {
            let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
            let shared_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3478);
            let mut relayer = RTCTurnRelayer::new(
                vec![local_addr],
                vec![RTCIceServer {
                    urls: vec![format!("turn:{}?transport=udp", shared_addr)],
                    username: "user".to_owned(),
                    credential: "pass".to_owned(),
                }],
                RTCIceTransportPolicy::Relay,
                None,
                crate::runtime::default_runtime().expect("test requires a runtime feature"),
                rtc::crypto::default_provider().expect("a built-in crypto provider for tests"),
            );

            relayer.gather().await.expect("TURN gather should start");
            let request = relayer.poll_write().expect("initial Allocate request");
            let mut request_msg = StunMessage::new();
            request_msg.raw = request.message.to_vec();
            request_msg.decode().expect("decode Allocate request");

            let response = build_turn_allocate_unauthorized(request_msg.transaction_id);
            let msg = tagged(local_addr, shared_addr, &response.raw);

            assert!(
                relayer.is_turn_message(&msg),
                "an Allocate response on the TURN client's four-tuple is TURN traffic"
            );
        });
    }

    /// The property a method-based guard could not have. A Binding response *is* TURN traffic when
    /// this client sent the request — as it would if the relayer ever gained STUN keepalives or
    /// consent freshness. Ownership follows the transaction id, so that stays correct without
    /// anyone remembering to revisit this function.
    #[test]
    fn binding_response_matching_an_outstanding_turn_transaction_routes_to_turn() {
        futures::executor::block_on(async {
            let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
            let shared_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3478);
            let mut relayer = RTCTurnRelayer::new(
                vec![local_addr],
                vec![RTCIceServer {
                    urls: vec![format!("turn:{}?transport=udp", shared_addr)],
                    username: "user".to_owned(),
                    credential: "pass".to_owned(),
                }],
                RTCIceTransportPolicy::Relay,
                None,
                crate::runtime::default_runtime().expect("test requires a runtime feature"),
                rtc::crypto::default_provider().expect("a built-in crypto provider for tests"),
            );

            relayer.gather().await.expect("TURN gather should start");
            relayer.poll_write().expect("initial Allocate request");

            let four_tuple = *relayer.clients.keys().next().expect("one TURN client");
            let tid = relayer
                .clients
                .get_mut(&four_tuple)
                .expect("the TURN client")
                .client
                .send_binding_request_to(
                    Instant::now(), // Exemption: usage in #test code
                    shared_addr,
                )
                .expect("TURN client sends its own Binding request");

            let response = build_binding_success(tid);
            let msg = tagged(local_addr, shared_addr, &response.raw);

            assert!(
                relayer.is_turn_message(&msg),
                "a Binding response the TURN client is waiting on is its own"
            );
        });
    }

    /// ChannelData carries no transaction id, so it must still be claimed on addresses alone.
    #[test]
    fn channel_data_on_exact_four_tuple_still_routes_to_turn() {
        futures::executor::block_on(async {
            let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
            let shared_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3478);
            let mut relayer = RTCTurnRelayer::new(
                vec![local_addr],
                vec![RTCIceServer {
                    urls: vec![format!("turn:{}?transport=udp", shared_addr)],
                    username: "user".to_owned(),
                    credential: "pass".to_owned(),
                }],
                RTCIceTransportPolicy::Relay,
                None,
                crate::runtime::default_runtime().expect("test requires a runtime feature"),
                rtc::crypto::default_provider().expect("a built-in crypto provider for tests"),
            );

            relayer.gather().await.expect("TURN gather should start");
            relayer.poll_write().expect("initial Allocate request");

            // Channel 0x4000, four payload bytes, padded to a 4-byte boundary.
            let channel_data = [0x40u8, 0x00, 0x00, 0x04, 0xde, 0xad, 0xbe, 0xef];
            assert!(
                ChannelData::is_channel_data(&channel_data),
                "test fixture must be valid ChannelData"
            );
            let msg = tagged(local_addr, shared_addr, &channel_data);

            assert!(
                relayer.is_turn_message(&msg),
                "ChannelData has no transaction id and must route by address"
            );
        });
    }
    fn tcp_relayer(url: &str) -> RTCTurnRelayer {
        let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
        RTCTurnRelayer::new(
            vec![local_addr],
            vec![RTCIceServer {
                urls: vec![url.to_owned()],
                username: "user".to_owned(),
                credential: "pass".to_owned(),
            }],
            RTCIceTransportPolicy::Relay,
            None,
            crate::runtime::default_runtime().expect("test requires a runtime feature"),
            rtc::crypto::default_provider().expect("a built-in crypto provider for tests"),
        )
    }

    fn turn_server() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3478)
    }

    /// What the host's TCP connection looks like: the kernel's source port, the server.
    fn tcp_tuple() -> FourTuple {
        FourTuple {
            local_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 40001),
            peer_addr: turn_server(),
        }
    }

    fn events(relayer: &mut RTCTurnRelayer) -> Vec<RTCTurnRelayEventOut> {
        std::iter::from_fn(|| relayer.poll_event()).collect()
    }

    /// `turn:…?transport=tcp` is no longer skipped: gathering asks the host for a connection
    /// and stays open until it hears back.
    #[test]
    fn a_tcp_turn_url_asks_for_a_connection_and_holds_gathering_open() {
        futures::executor::block_on(async {
            let mut relayer = tcp_relayer(&format!("turn:{}?transport=tcp", turn_server()));
            relayer.gather().await.expect("gather");
            assert!(relayer.poll_write().is_none(), "nothing goes out over UDP");
            let events = events(&mut relayer);
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    RTCTurnRelayEventOut::ConnectTcp { server, .. } if *server == turn_server()
                )),
                "the host must be asked to connect: {events:?}"
            );
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, RTCTurnRelayEventOut::TurnGatheringComplete)),
                "gathering cannot be complete while a connection is pending"
            );
            assert_eq!(relayer.state(), RTCIceGatheringState::Gathering);
        });
    }

    fn connect_id(relayer: &mut RTCTurnRelayer) -> u64 {
        events(relayer)
            .into_iter()
            .find_map(|e| match e {
                RTCTurnRelayEventOut::ConnectTcp { id, .. } => Some(id),
                _ => None,
            })
            .expect("a ConnectTcp event")
    }

    /// Once connected, the Allocate travels over TCP and asks for a UDP relay (RFC 8656 §7.1).
    #[test]
    fn a_connected_server_is_asked_for_a_udp_relay_over_tcp() {
        futures::executor::block_on(async {
            let mut relayer = tcp_relayer(&format!("turn:{}?transport=tcp", turn_server()));
            relayer.gather().await.expect("gather");
            let id = connect_id(&mut relayer);

            let registered = relayer
                .on_tcp_connected(id, Ok(tcp_tuple()))
                .expect("a client over the connection");
            assert_eq!(
                registered,
                Some(tcp_tuple()),
                "the host registers this stream"
            );

            let allocate = relayer
                .poll_write()
                .expect("an Allocate over the connection");
            assert_eq!(
                allocate.transport.transport_protocol,
                TransportProtocol::TCP
            );
            assert_eq!(FourTuple::from(&allocate.transport), tcp_tuple());
            let mut msg = StunMessage::new();
            msg.raw = allocate.message.to_vec();
            msg.decode().expect("decode Allocate");
            let mut requested = rtc::turn::proto::reqtrans::RequestedTransport::default();
            rtc::stun::message::Getter::get_from(&mut requested, &msg)
                .expect("REQUESTED-TRANSPORT");
            assert_eq!(requested.protocol, rtc::turn::proto::PROTO_UDP);

            // Its answer, arriving on the stream, is the relayer's.
            let response = build_turn_allocate_unauthorized(msg.transaction_id);
            let mut reply = tagged(tcp_tuple().local_addr, turn_server(), &response.raw);
            reply.transport.transport_protocol = TransportProtocol::TCP;
            assert!(relayer.is_turn_message(&reply));
        });
    }

    /// A connection that fails, or a server that never answers, ends that server's part in
    /// gathering; a result nobody is waiting for any more — from before an ICE restart, say —
    /// changes nothing.
    #[test]
    fn a_failed_connection_completes_gathering_and_a_stale_one_is_ignored() {
        futures::executor::block_on(async {
            let mut relayer = tcp_relayer(&format!("turn:{}?transport=tcp", turn_server()));
            relayer.gather().await.expect("gather");
            let id = connect_id(&mut relayer);

            let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
            assert_eq!(relayer.on_tcp_connected(id, Err(refused)).unwrap(), None);
            assert!(
                events(&mut relayer)
                    .iter()
                    .any(|e| matches!(e, RTCTurnRelayEventOut::TurnGatheringComplete)),
                "no relay from this server, and gathering moves on"
            );

            assert_eq!(
                relayer.on_tcp_connected(id, Ok(tcp_tuple())).unwrap(),
                None,
                "an answer to a connect nobody is waiting for is not kept"
            );
            assert!(relayer.poll_write().is_none());
        });
    }

    /// The same four-tuple over UDP is not the TCP client's traffic.
    #[test]
    fn udp_traffic_never_routes_to_a_tcp_client() {
        futures::executor::block_on(async {
            let mut relayer = tcp_relayer(&format!("turn:{}?transport=tcp", turn_server()));
            relayer.gather().await.expect("gather");
            let id = connect_id(&mut relayer);
            relayer.on_tcp_connected(id, Ok(tcp_tuple())).unwrap();
            let _ = relayer.poll_write();

            let channel_data = [0x40, 0x00, 0x00, 0x00];
            let over_udp = tagged(tcp_tuple().local_addr, turn_server(), &channel_data);
            assert!(
                !relayer.is_turn_message(&over_udp),
                "a datagram cannot belong to a stream"
            );
        });
    }

    /// TURN over TLS is the next change; until then `turns:` is skipped as before.
    #[test]
    fn turns_urls_are_still_skipped() {
        futures::executor::block_on(async {
            let mut relayer = tcp_relayer(&format!("turns:{}?transport=tcp", turn_server()));
            relayer.gather().await.expect("gather");
            let events = events(&mut relayer);
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, RTCTurnRelayEventOut::ConnectTcp { .. })),
                "{events:?}"
            );
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, RTCTurnRelayEventOut::TurnGatheringComplete)),
                "{events:?}"
            );
        });
    }

    /// Beside an established UDP client, a credential-only change re-signs rather than
    /// regathers. A TCP connect still in flight is a client still mid-Allocate, and must not
    /// then come up on the old password: those are regathered, never re-signed.
    #[test]
    fn a_credential_change_during_a_tcp_connect_is_not_applied_to_the_old_credentials() {
        futures::executor::block_on(async {
            let udp_url = format!("turn:{}?transport=udp", turn_server());
            let tcp_url = format!("turn:{}?transport=tcp", turn_server());
            let servers = |credential: &str| {
                vec![RTCIceServer {
                    urls: vec![udp_url.clone(), tcp_url.clone()],
                    username: "user".to_owned(),
                    credential: credential.to_owned(),
                }]
            };
            let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000);
            let mut relayer = RTCTurnRelayer::new(
                vec![local_addr],
                servers("pass"),
                RTCIceTransportPolicy::Relay,
                None,
                crate::runtime::default_runtime().expect("test requires a runtime feature"),
                rtc::crypto::default_provider().expect("a built-in crypto provider for tests"),
            );
            relayer.gather().await.expect("gather");
            let id = connect_id(&mut relayer);

            // Bring the UDP client's allocation up: 401, the signed retry, success.
            let answer = |relayer: &mut RTCTurnRelayer, response: StunMessage| {
                relayer
                    .handle_read(tagged(local_addr, turn_server(), &response.raw))
                    .expect("the relayer takes its answer");
            };
            let first = relayer.poll_write().expect("the UDP Allocate");
            let mut request = StunMessage::new();
            request.raw = first.message.to_vec();
            request.decode().unwrap();
            answer(
                &mut relayer,
                build_turn_allocate_unauthorized(request.transaction_id),
            );
            let retry = relayer.poll_write().expect("the signed Allocate");
            request.raw = retry.message.to_vec();
            request.decode().unwrap();
            let mut success = StunMessage::new();
            success
                .build(&[
                    Box::new(request.transaction_id),
                    Box::new(MessageType::new(
                        rtc::stun::message::METHOD_ALLOCATE,
                        CLASS_SUCCESS_RESPONSE,
                    )),
                    Box::new(rtc::turn::proto::relayaddr::RelayedAddress {
                        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                        port: 49999,
                    }),
                    Box::new(rtc::turn::proto::lifetime::Lifetime(
                        std::time::Duration::from_secs(600),
                    )),
                ])
                .unwrap();
            answer(&mut relayer, success);
            let _ = events(&mut relayer);
            assert!(
                relayer
                    .contains_local_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 49999))
            );

            relayer.update_configuration(servers("rotated"), RTCIceTransportPolicy::Relay);
            assert_eq!(
                relayer.on_tcp_connected(id, Ok(tcp_tuple())).unwrap(),
                None,
                "a client built from that connect would carry the old password"
            );
        });
    }

    /// Reconfiguring away from a server lets go of its connection, for the host to close.
    #[test]
    fn a_reconfiguration_releases_the_tcp_connection() {
        futures::executor::block_on(async {
            let mut relayer = tcp_relayer(&format!("turn:{}?transport=tcp", turn_server()));
            relayer.gather().await.expect("gather");
            let id = connect_id(&mut relayer);
            relayer.on_tcp_connected(id, Ok(tcp_tuple())).unwrap();

            relayer.update_configuration(vec![], RTCIceTransportPolicy::All);
            assert_eq!(relayer.take_released_tcp_streams(), vec![tcp_tuple()]);
            assert!(relayer.take_released_tcp_streams().is_empty(), "once only");
        });
    }
}
