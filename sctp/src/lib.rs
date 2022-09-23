//! SCTP transport protocol support for Tokio
//!
//! [SCTP](https://en.wikipedia.org/wiki/Stream_Control_Transmission_Protocol), defined in RFC 4960,
//! is used in WebRTC for peer-to-peer arbitrary data delivery across browsers. WebRTC uses it as an
//! application layer protocol which runs over our DTLS connection.
//!
//! SCTP gives you streams and each stream can be configured independently. WebRTC data channels
//! are just thin abstractions around them. The settings around durability and ordering are just
//! passed right into the SCTP Agent.
//!

#![warn(rust_2018_idioms)]
#![allow(dead_code)]

use std::time::Duration;

mod association;
mod broadcast;
mod endpoint;
mod mutex;
mod recv_stream;
mod send_stream;
mod udp;
mod work_limiter;

pub use proto::{
    AssociationError, Chunk, ClientConfig, ConnectError, EndpointConfig, Error, ErrorCauseCode,
    ServerConfig, StreamId, Transmit, TransportConfig, PayloadProtocolIdentifier, ReliabilityType
};

pub use crate::association::{Association, Connecting, IncomingStreams, NewAssociation, Opening};
pub use crate::endpoint::{Endpoint, Incoming};
pub use crate::recv_stream::{
    Read, ReadChunk, ReadChunks, ReadError, ReadExact, ReadExactError, ReadToEnd, ReadToEndError,
    RecvStream,
};
pub use crate::send_stream::{SendStream, StoppedError, WriteError};

#[derive(Debug)]
enum AssociationEvent {
    Close {
        error_code: proto::ErrorCauseCode,
        reason: bytes::Bytes,
    },
    Proto(proto::AssociationEvent),
}

#[derive(Debug)]
enum EndpointEvent {
    Proto(proto::EndpointEvent),
    Transmit(proto::Transmit),
}

/// Maximum number of datagrams processed in send/recv calls to make before moving on to other processing
///
/// This helps ensure we don't starve anything when the CPU is slower than the link.
/// Value is selected by picking a low number which didn't degrade throughput in benchmarks.
const IO_LOOP_BOUND: usize = 160;

/// The maximum amount of time that should be spent in `recvmsg()` calls per endpoint iteration
///
/// 50us are chosen so that an endpoint iteration with a 50us sendmsg limit blocks
/// the runtime for a maximum of about 100us.
/// Going much lower does not yield any noticeable difference, since a single `recvmmsg`
/// batch of size 32 was observed to take 30us on some systems.
const RECV_TIME_BOUND: Duration = Duration::from_micros(50);

/// The maximum amount of time that should be spent in `sendmsg()` calls per endpoint iteration
const SEND_TIME_BOUND: Duration = Duration::from_micros(50);
