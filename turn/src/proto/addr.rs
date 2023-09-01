#[cfg(test)]
mod addr_test;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use super::*;

/// `Addr` is `ip:port`.
#[derive(PartialEq, Eq, Debug)]
pub struct Addr {
    ip: IpAddr,
    port: u16,
}

impl Default for Addr {
    fn default() -> Self {
        Addr {
            ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            port: 0,
        }
    }
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

impl Addr {
    /// Returns this network.
    pub fn network(&self) -> String {
        "turn".to_owned()
    }

    /// Creates a new [`Addr`] from `n`.
    pub fn from_socket_addr(n: &SocketAddr) -> Self {
        let ip = n.ip();
        let port = n.port();

        Addr { ip, port }
    }

    /// Returns `true` if the `other` has the same IP address.
    pub fn equal_ip(&self, other: &Addr) -> bool {
        self.ip == other.ip
    }
}

// FiveTuple represents 5-TUPLE value.
#[derive(PartialEq, Eq, Default)]
pub struct FiveTuple {
    pub client: Addr,
    pub server: Addr,
    pub proto: Protocol,
}

impl fmt::Display for FiveTuple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}->{} ({})", self.client, self.server, self.proto)
    }
}
