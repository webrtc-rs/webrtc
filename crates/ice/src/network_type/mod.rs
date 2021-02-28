#[cfg(test)]
mod network_type_test;

use crate::errors::*;

use util::Error;

use std::fmt;
use std::net::IpAddr;

pub(crate) const UDP: &str = "udp";
pub(crate) const TCP: &str = "tcp";

// NetworkType represents the type of network
#[derive(PartialEq, Debug, Copy, Clone, Eq, Hash)]
pub enum NetworkType {
    // NetworkTypeUDP4 indicates UDP over IPv4.
    UDP4 = 0,

    // NetworkTypeUDP6 indicates UDP over IPv6.
    UDP6 = 1,

    // NetworkTypeTCP4 indicates TCP over IPv4.
    TCP4 = 2,

    // NetworkTypeTCP6 indicates TCP over IPv6.
    TCP6 = 3,
}

impl From<u8> for NetworkType {
    fn from(v: u8) -> NetworkType {
        match v {
            0 => NetworkType::UDP4,
            1 => NetworkType::UDP6,
            2 => NetworkType::UDP4,
            _ => NetworkType::TCP6,
        }
    }
}

impl fmt::Display for NetworkType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            NetworkType::UDP4 => "udp4",
            NetworkType::UDP6 => "udp6",
            NetworkType::TCP4 => "tcp4",
            NetworkType::TCP6 => "tcp6",
        };
        write!(f, "{}", s)
    }
}

impl Default for NetworkType {
    fn default() -> Self {
        NetworkType::UDP4
    }
}

impl NetworkType {
    // is_udp returns true when network is UDP4 or UDP6.
    pub fn is_udp(&self) -> bool {
        *self == NetworkType::UDP4 || *self == NetworkType::UDP6
    }

    // is_tcp returns true when network is TCP4 or TCP6.
    pub fn is_tcp(&self) -> bool {
        *self == NetworkType::TCP4 || *self == NetworkType::TCP6
    }

    // network_short returns the short network description
    pub fn network_short(&self) -> String {
        match *self {
            NetworkType::UDP4 | NetworkType::UDP6 => UDP.to_owned(),
            NetworkType::TCP4 | NetworkType::TCP6 => TCP.to_owned(),
        }
    }

    // IsReliable returns true if the network is reliable
    pub fn is_reliable(&self) -> bool {
        match *self {
            NetworkType::UDP4 | NetworkType::UDP6 => false,
            NetworkType::TCP4 | NetworkType::TCP6 => true,
        }
    }

    // IsIPv4 returns whether the network type is IPv4 or not.
    pub fn is_ipv4(&self) -> bool {
        match *self {
            NetworkType::UDP4 | NetworkType::TCP4 => true,
            NetworkType::UDP6 | NetworkType::TCP6 => false,
        }
    }

    // IsIPv6 returns whether the network type is IPv6 or not.
    pub fn is_ipv6(&self) -> bool {
        match *self {
            NetworkType::UDP4 | NetworkType::TCP4 => false,
            NetworkType::UDP6 | NetworkType::TCP6 => true,
        }
    }
}

// determine_network_type determines the type of network based on
// the short network string and an IP address.
pub(crate) fn determine_network_type(network: &str, ip: &IpAddr) -> Result<NetworkType, Error> {
    let ipv4 = ip.is_ipv4();
    let net = network.to_lowercase();
    if net.starts_with(UDP) {
        if ipv4 {
            Ok(NetworkType::UDP4)
        } else {
            Ok(NetworkType::UDP6)
        }
    } else if net.starts_with(TCP) {
        if ipv4 {
            Ok(NetworkType::TCP4)
        } else {
            Ok(NetworkType::TCP6)
        }
    } else {
        Err(ERR_DETERMINE_NETWORK_TYPE.to_owned())
    }
}
