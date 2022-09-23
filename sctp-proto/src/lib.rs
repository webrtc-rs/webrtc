//! Low-level protocol logic for the SCTP protocol
//!
//! sctp-proto contains a fully deterministic implementation of SCTP protocol logic. It contains
//! no networking code and does not get any relevant timestamps from the operating system. Most
//! users may want to use the futures-based sctp-async API instead.
//!
//! The sctp-proto API might be of interest if you want to use it from a C or C++ project
//! through C bindings or if you want to use a different event loop than the one tokio provides.
//!
//! The most important types are `Endpoint`, which conceptually represents the protocol state for
//! a single socket and mostly manages configuration and dispatches incoming datagrams to the
//! related `Association`. `Association` types contain the bulk of the protocol logic related to
//! managing a single association and all the related state (such as streams).

#![warn(rust_2018_idioms)]
#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

use bytes::Bytes;
use std::time::Instant;
use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    ops,
};

mod association;
pub use crate::association::{
    stats::AssociationStats,
    stream::{ReliabilityType, Stream, StreamEvent, StreamId, StreamState},
    Association, AssociationError, Event,
};

pub(crate) mod chunk;
pub use crate::chunk::{
    chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier},
    ErrorCauseCode,
};

mod config;
pub use crate::config::{ClientConfig, EndpointConfig, ServerConfig, TransportConfig};

mod endpoint;
pub use crate::endpoint::{AssociationHandle, ConnectError, DatagramEvent, Endpoint};

mod error;
pub use crate::error::Error;

mod packet;

mod shared;
pub use crate::shared::{AssociationEvent, AssociationId, EcnCodepoint, EndpointEvent};

pub(crate) mod param;

pub(crate) mod queue;
pub use crate::queue::reassembly_queue::{Chunk, Chunks};

pub(crate) mod util;

/// Whether an endpoint was the initiator of an association
#[cfg_attr(feature = "arbitrary", derive(Arbitrary))]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Side {
    /// The initiator of an association
    Client = 0,
    /// The acceptor of an association
    Server = 1,
}

impl Default for Side {
    fn default() -> Self {
        Side::Client
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Side::Client => "Client",
            Side::Server => "Server",
        };
        write!(f, "{}", s)
    }
}

impl Side {
    #[inline]
    /// Shorthand for `self == Side::Client`
    pub fn is_client(self) -> bool {
        self == Side::Client
    }

    #[inline]
    /// Shorthand for `self == Side::Server`
    pub fn is_server(self) -> bool {
        self == Side::Server
    }
}

impl ops::Not for Side {
    type Output = Side;
    fn not(self) -> Side {
        match self {
            Side::Client => Side::Server,
            Side::Server => Side::Client,
        }
    }
}

use crate::packet::PartialDecode;

/// Payload in Incoming/outgoing Transmit
#[derive(Debug)]
pub enum Payload {
    PartialDecode(PartialDecode),
    RawEncode(Vec<Bytes>),
}

/// Incoming/outgoing Transmit
#[derive(Debug)]
pub struct Transmit {
    /// Received/Sent time
    pub now: Instant,
    /// The socket this datagram should be sent to
    pub remote: SocketAddr,
    /// Explicit congestion notification bits to set on the packet
    pub ecn: Option<EcnCodepoint>,
    /// Optional local IP address for the datagram
    pub local_ip: Option<IpAddr>,
    /// Payload of the datagram
    pub payload: Payload,
}
