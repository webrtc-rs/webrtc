#[cfg(test)]
mod five_tuple_test;

use std::fmt;
use std::net::{Ipv4Addr, SocketAddr};

use crate::proto::*;

/// `FiveTuple` is the combination (client IP address and port, server IP
/// address and port, and transport protocol (currently one of UDP,
/// TCP, or TLS)) used to communicate between the client and the
/// server.  The 5-tuple uniquely identifies this communication
/// stream.  The 5-tuple also uniquely identifies the Allocation on
/// the server.
#[derive(PartialEq, Eq, Clone, Copy, Hash)]
pub struct FiveTuple {
    pub protocol: Protocol,
    pub src_addr: SocketAddr,
    pub dst_addr: SocketAddr,
}

impl Default for FiveTuple {
    fn default() -> Self {
        FiveTuple {
            protocol: PROTO_UDP,
            src_addr: SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
            dst_addr: SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
        }
    }
}

impl fmt::Display for FiveTuple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}_{}", self.protocol, self.src_addr, self.dst_addr)
    }
}

impl fmt::Debug for FiveTuple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FiveTuple")
            .field("protocol", &self.protocol)
            .field("src_addr", &self.src_addr)
            .field("dst_addr", &self.dst_addr)
            .finish()
    }
}
