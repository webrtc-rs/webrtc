#[cfg(test)]
mod five_tuple_test;

use crate::proto::*;

use std::fmt;
use std::net::SocketAddr;

// FiveTuple is the combination (client IP address and port, server IP
// address and port, and transport protocol (currently one of UDP,
// TCP, or TLS)) used to communicate between the client and the
// server.  The 5-tuple uniquely identifies this communication
// stream.  The 5-tuple also uniquely identifies the Allocation on
// the server.
#[derive(PartialEq)]
pub struct FiveTuple {
    pub protocol: Protocol,
    pub src_addr: SocketAddr,
    pub dst_addr: SocketAddr,
}

impl fmt::Display for FiveTuple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}_{}", self.protocol, self.src_addr, self.dst_addr)
    }
}

impl FiveTuple {
    // fingerprint is the identity of a FiveTuple
    pub fn fingerprint(&self) -> String {
        self.to_string()
    }
}
