#[cfg(test)]
mod endpoint_test;

use std::{
    collections::{HashMap, VecDeque},
    fmt, iter,
    net::{IpAddr, SocketAddr},
    ops::{Index, IndexMut},
    sync::Arc,
    time::Instant,
};

use crate::association::Association;
use crate::chunk::chunk_type::CT_INIT;
use crate::config::{ClientConfig, EndpointConfig, ServerConfig, TransportConfig};
use crate::packet::PartialDecode;
use crate::shared::{
    AssociationEvent, AssociationEventInner, AssociationId, EndpointEvent, EndpointEventInner,
};
use crate::util::{AssociationIdGenerator, RandomAssociationIdGenerator};
use crate::{EcnCodepoint, Payload, Transmit};

use bytes::Bytes;
use fxhash::FxHashMap;
use log::{debug, trace};
use rand::{rngs::StdRng, SeedableRng};
use slab::Slab;
use thiserror::Error;

/// The main entry point to the library
///
/// This object performs no I/O whatsoever. Instead, it generates a stream of packets to send via
/// `poll_transmit`, and consumes incoming packets and association-generated events via `handle` and
/// `handle_event`.
pub struct Endpoint {
    rng: StdRng,
    transmits: VecDeque<Transmit>,
    /// Identifies associations based on the INIT Dst AID the peer utilized
    ///
    /// Uses a standard `HashMap` to protect against hash collision attacks.
    association_ids_init: HashMap<AssociationId, AssociationHandle>,
    /// Identifies associations based on locally created CIDs
    ///
    /// Uses a cheaper hash function since keys are locally created
    association_ids: FxHashMap<AssociationId, AssociationHandle>,

    associations: Slab<AssociationMeta>,
    local_cid_generator: Box<dyn AssociationIdGenerator>,
    config: Arc<EndpointConfig>,
    server_config: Option<Arc<ServerConfig>>,
    /// Whether incoming associations should be unconditionally rejected by a server
    ///
    /// Equivalent to a `ServerConfig.accept_buffer` of `0`, but can be changed after the endpoint is constructed.
    reject_new_associations: bool,
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Endpoint<T>")
            .field("rng", &self.rng)
            .field("transmits", &self.transmits)
            .field("association_ids_initial", &self.association_ids_init)
            .field("association_ids", &self.association_ids)
            .field("associations", &self.associations)
            .field("config", &self.config)
            .field("server_config", &self.server_config)
            .field("reject_new_associations", &self.reject_new_associations)
            .finish()
    }
}

impl Endpoint {
    /// Create a new endpoint
    ///
    /// Returns `Err` if the configuration is invalid.
    pub fn new(config: Arc<EndpointConfig>, server_config: Option<Arc<ServerConfig>>) -> Self {
        Self {
            rng: StdRng::from_entropy(),
            transmits: VecDeque::new(),
            association_ids_init: HashMap::default(),
            association_ids: FxHashMap::default(),
            associations: Slab::new(),
            local_cid_generator: (config.aid_generator_factory.as_ref())(),
            reject_new_associations: false,
            config,
            server_config,
        }
    }

    /// Get the next packet to transmit
    #[must_use]
    pub fn poll_transmit(&mut self) -> Option<Transmit> {
        self.transmits.pop_front()
    }

    /// Replace the server configuration, affecting new incoming associations only
    pub fn set_server_config(&mut self, server_config: Option<Arc<ServerConfig>>) {
        self.server_config = server_config;
    }

    /// Process `EndpointEvent`s emitted from related `Association`s
    ///
    /// In turn, processing this event may return a `AssociationEvent` for the same `Association`.
    pub fn handle_event(
        &mut self,
        ch: AssociationHandle,
        event: EndpointEvent,
    ) -> Option<AssociationEvent> {
        match event.0 {
            EndpointEventInner::Drained => {
                let conn = self.associations.remove(ch.0);
                self.association_ids_init.remove(&conn.init_cid);
                for cid in conn.loc_cids.values() {
                    self.association_ids.remove(cid);
                }
            }
        }
        None
    }

    /// Process an incoming UDP datagram
    pub fn handle(
        &mut self,
        now: Instant,
        remote: SocketAddr,
        local_ip: Option<IpAddr>,
        ecn: Option<EcnCodepoint>,
        data: Bytes,
    ) -> Option<(AssociationHandle, DatagramEvent)> {
        let partial_decode = match PartialDecode::unmarshal(&data) {
            Ok(x) => x,
            Err(err) => {
                trace!("malformed header: {}", err);
                return None;
            }
        };

        //
        // Handle packet on existing association, if any
        //
        let dst_cid = partial_decode.common_header.verification_tag;
        let known_ch = if dst_cid > 0 {
            self.association_ids.get(&dst_cid).cloned()
        } else {
            //TODO: improve INIT handling for DoS attack
            if partial_decode.first_chunk_type == CT_INIT {
                if let Some(dst_cid) = partial_decode.initiate_tag {
                    self.association_ids.get(&dst_cid).cloned()
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(ch) = known_ch {
            return Some((
                ch,
                DatagramEvent::AssociationEvent(AssociationEvent(AssociationEventInner::Datagram(
                    Transmit {
                        now,
                        remote,
                        ecn,
                        payload: Payload::PartialDecode(partial_decode),
                        local_ip,
                    },
                ))),
            ));
        }

        //
        // Potentially create a new association
        //
        self.handle_first_packet(now, remote, local_ip, ecn, partial_decode)
            .map(|(ch, a)| (ch, DatagramEvent::NewAssociation(a)))
    }

    /// Initiate an Association
    pub fn connect(
        &mut self,
        config: ClientConfig,
        remote: SocketAddr,
    ) -> Result<(AssociationHandle, Association), ConnectError> {
        if self.is_full() {
            return Err(ConnectError::TooManyAssociations);
        }
        if remote.port() == 0 {
            return Err(ConnectError::InvalidRemoteAddress(remote));
        }

        let remote_aid = RandomAssociationIdGenerator::new().generate_aid();
        let local_aid = self.new_aid();

        let (ch, conn) = self.add_association(
            remote_aid,
            local_aid,
            remote,
            None,
            Instant::now(),
            None,
            config.transport,
        );
        Ok((ch, conn))
    }

    fn new_aid(&mut self) -> AssociationId {
        loop {
            let aid = self.local_cid_generator.generate_aid();
            if !self.association_ids.contains_key(&aid) {
                break aid;
            }
        }
    }

    fn handle_first_packet(
        &mut self,
        now: Instant,
        remote: SocketAddr,
        local_ip: Option<IpAddr>,
        ecn: Option<EcnCodepoint>,
        partial_decode: PartialDecode,
    ) -> Option<(AssociationHandle, Association)> {
        if partial_decode.first_chunk_type != CT_INIT
            || (partial_decode.first_chunk_type == CT_INIT && partial_decode.initiate_tag.is_none())
        {
            debug!("refusing first packet with Non-INIT or emtpy initial_tag INIT");
            return None;
        }

        let server_config = self.server_config.as_ref().unwrap();

        if self.associations.len() >= server_config.concurrent_associations as usize
            || self.reject_new_associations
            || self.is_full()
        {
            debug!("refusing association");
            //TODO: self.initial_close();
            return None;
        }

        let server_config = server_config.clone();
        let transport_config = server_config.transport.clone();

        let remote_aid = *partial_decode.initiate_tag.as_ref().unwrap();
        let local_aid = self.new_aid();

        let (ch, mut conn) = self.add_association(
            remote_aid,
            local_aid,
            remote,
            local_ip,
            now,
            Some(server_config),
            transport_config,
        );

        conn.handle_event(AssociationEvent(AssociationEventInner::Datagram(
            Transmit {
                now,
                remote,
                ecn,
                payload: Payload::PartialDecode(partial_decode),
                local_ip,
            },
        )));

        Some((ch, conn))
    }

    fn add_association(
        &mut self,
        remote_aid: AssociationId,
        local_aid: AssociationId,
        remote_addr: SocketAddr,
        local_ip: Option<IpAddr>,
        now: Instant,
        server_config: Option<Arc<ServerConfig>>,
        transport_config: Arc<TransportConfig>,
    ) -> (AssociationHandle, Association) {
        let conn = Association::new(
            server_config,
            transport_config,
            local_aid,
            remote_addr,
            local_ip,
            now,
        );

        let id = self.associations.insert(AssociationMeta {
            init_cid: remote_aid,
            cids_issued: 0,
            loc_cids: iter::once((0, local_aid)).collect(),
            initial_remote: remote_addr,
        });

        let ch = AssociationHandle(id);
        self.association_ids.insert(local_aid, ch);

        (ch, conn)
    }

    /// Unconditionally reject future incoming associations
    pub fn reject_new_associations(&mut self) {
        self.reject_new_associations = true;
    }

    /// Access the configuration used by this endpoint
    pub fn config(&self) -> &EndpointConfig {
        &self.config
    }

    /// Whether we've used up 3/4 of the available AID space
    fn is_full(&self) -> bool {
        (((u32::MAX >> 1) + (u32::MAX >> 2)) as usize) < self.association_ids.len()
    }
}

#[derive(Debug)]
pub(crate) struct AssociationMeta {
    init_cid: AssociationId,
    /// Number of local association IDs.
    cids_issued: u64,
    loc_cids: FxHashMap<u64, AssociationId>,
    /// Remote address the association began with
    ///
    /// Only needed to support associations with zero-length AIDs, which cannot migrate, so we don't
    /// bother keeping it up to date.
    initial_remote: SocketAddr,
}

/// Internal identifier for an `Association` currently associated with an endpoint
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct AssociationHandle(pub usize);

impl From<AssociationHandle> for usize {
    fn from(x: AssociationHandle) -> usize {
        x.0
    }
}

impl Index<AssociationHandle> for Slab<AssociationMeta> {
    type Output = AssociationMeta;
    fn index(&self, ch: AssociationHandle) -> &AssociationMeta {
        &self[ch.0]
    }
}

impl IndexMut<AssociationHandle> for Slab<AssociationMeta> {
    fn index_mut(&mut self, ch: AssociationHandle) -> &mut AssociationMeta {
        &mut self[ch.0]
    }
}

/// Event resulting from processing a single datagram
#[allow(clippy::large_enum_variant)] // Not passed around extensively
pub enum DatagramEvent {
    /// The datagram is redirected to its `Association`
    AssociationEvent(AssociationEvent),
    /// The datagram has resulted in starting a new `Association`
    NewAssociation(Association),
}

/// Errors in the parameters being used to create a new association
///
/// These arise before any I/O has been performed.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// The endpoint can no longer create new associations
    ///
    /// Indicates that a necessary component of the endpoint has been dropped or otherwise disabled.
    #[error("endpoint stopping")]
    EndpointStopping,
    /// The number of active associations on the local endpoint is at the limit
    ///
    /// Try using longer association IDs.
    #[error("too many associations")]
    TooManyAssociations,
    /// The domain name supplied was malformed
    #[error("invalid DNS name: {0}")]
    InvalidDnsName(String),
    /// The remote [`SocketAddr`] supplied was malformed
    ///
    /// Examples include attempting to connect to port 0, or using an inappropriate address family.
    #[error("invalid remote address: {0}")]
    InvalidRemoteAddress(SocketAddr),
    /// No default client configuration was set up
    ///
    /// Use `Endpoint::connect_with` to specify a client configuration.
    #[error("no default client config")]
    NoDefaultClientConfig,
}
