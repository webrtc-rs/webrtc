#[cfg(test)]
mod peeraddr_test;

use std::fmt;
use std::net::{IpAddr, Ipv4Addr};

use stun::attributes::*;
use stun::message::*;
use stun::xoraddr::*;

/// `PeerAddress` implements `XOR-PEER-ADDRESS` attribute.
///
/// The `XOR-PEER-ADDRESS` specifies the address and port of the peer as
/// seen from the TURN server. (For example, the peer's server-reflexive
/// transport address if the peer is behind a NAT.)
///
/// [RFC 5766 Section 14.3](https://www.rfc-editor.org/rfc/rfc5766#section-14.3).
#[derive(PartialEq, Eq, Debug)]
pub struct PeerAddress {
    pub ip: IpAddr,
    pub port: u16,
}

impl Default for PeerAddress {
    fn default() -> Self {
        PeerAddress {
            ip: IpAddr::V4(Ipv4Addr::from(0)),
            port: 0,
        }
    }
}

impl fmt::Display for PeerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.ip {
            IpAddr::V4(_) => write!(f, "{}:{}", self.ip, self.port),
            IpAddr::V6(_) => write!(f, "[{}]:{}", self.ip, self.port),
        }
    }
}

impl Setter for PeerAddress {
    /// Adds `XOR-PEER-ADDRESS` to message.
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        let a = XorMappedAddress {
            ip: self.ip,
            port: self.port,
        };
        a.add_to_as(m, ATTR_XOR_PEER_ADDRESS)
    }
}

impl Getter for PeerAddress {
    /// Decodes `XOR-PEER-ADDRESS` from message.
    fn get_from(&mut self, m: &Message) -> Result<(), stun::Error> {
        let mut a = XorMappedAddress::default();
        a.get_from_as(m, ATTR_XOR_PEER_ADDRESS)?;
        self.ip = a.ip;
        self.port = a.port;
        Ok(())
    }
}

/// `PeerAddress` implements `XOR-PEER-ADDRESS` attribute.
///
/// The `XOR-PEER-ADDRESS` specifies the address and port of the peer as
/// seen from the TURN server. (For example, the peer's server-reflexive
/// transport address if the peer is behind a NAT.)
///
/// [RFC 5766 Section 14.3](https://www.rfc-editor.org/rfc/rfc5766#section-14.3).
pub type XorPeerAddress = PeerAddress;
