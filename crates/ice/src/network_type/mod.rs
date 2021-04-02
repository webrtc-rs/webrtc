#[cfg(test)]
mod network_type_test;

use crate::errors::*;

use util::Error;

use std::fmt;
use std::net::IpAddr;

pub(crate) const UDP: &str = "udp";
pub(crate) const TCP: &str = "tcp";

pub fn supported_network_types() -> Vec<NetworkType> {
    vec![
        NetworkType::Udp4,
        NetworkType::Udp6,
        //NetworkType::TCP4,
        //NetworkType::TCP6,
    ]
}

// NetworkType represents the type of network
#[derive(PartialEq, Debug, Copy, Clone, Eq, Hash)]
pub enum NetworkType {
    Unspecified,

    // NetworkTypeUDP4 indicates UDP over IPv4.
    Udp4,

    // NetworkTypeUDP6 indicates UDP over IPv6.
    Udp6,

    // NetworkTypeTCP4 indicates TCP over IPv4.
    Tcp4,

    // NetworkTypeTCP6 indicates TCP over IPv6.
    Tcp6,
}

impl From<u8> for NetworkType {
    fn from(v: u8) -> NetworkType {
        match v {
            1 => NetworkType::Udp4,
            2 => NetworkType::Udp6,
            3 => NetworkType::Tcp4,
            4 => NetworkType::Tcp6,
            _ => NetworkType::Unspecified,
        }
    }
}

impl fmt::Display for NetworkType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            NetworkType::Udp4 => "udp4",
            NetworkType::Udp6 => "udp6",
            NetworkType::Tcp4 => "tcp4",
            NetworkType::Tcp6 => "tcp6",
            NetworkType::Unspecified => "unspecified",
        };
        write!(f, "{}", s)
    }
}

impl Default for NetworkType {
    fn default() -> Self {
        NetworkType::Unspecified
    }
}

impl NetworkType {
    // is_udp returns true when network is UDP4 or UDP6.
    pub fn is_udp(&self) -> bool {
        *self == NetworkType::Udp4 || *self == NetworkType::Udp6
    }

    // is_tcp returns true when network is TCP4 or TCP6.
    pub fn is_tcp(&self) -> bool {
        *self == NetworkType::Tcp4 || *self == NetworkType::Tcp6
    }

    // network_short returns the short network description
    pub fn network_short(&self) -> String {
        match *self {
            NetworkType::Udp4 | NetworkType::Udp6 => UDP.to_owned(),
            NetworkType::Tcp4 | NetworkType::Tcp6 => TCP.to_owned(),
            _ => "Unspecified".to_owned(),
        }
    }

    // IsReliable returns true if the network is reliable
    pub fn is_reliable(&self) -> bool {
        match *self {
            NetworkType::Udp4 | NetworkType::Udp6 => false,
            NetworkType::Tcp4 | NetworkType::Tcp6 => true,
            _ => false,
        }
    }

    // IsIPv4 returns whether the network type is IPv4 or not.
    pub fn is_ipv4(&self) -> bool {
        match *self {
            NetworkType::Udp4 | NetworkType::Tcp4 => true,
            NetworkType::Udp6 | NetworkType::Tcp6 => false,
            _ => false,
        }
    }

    // IsIPv6 returns whether the network type is IPv6 or not.
    pub fn is_ipv6(&self) -> bool {
        match *self {
            NetworkType::Udp4 | NetworkType::Tcp4 => false,
            NetworkType::Udp6 | NetworkType::Tcp6 => true,
            _ => false,
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
            Ok(NetworkType::Udp4)
        } else {
            Ok(NetworkType::Udp6)
        }
    } else if net.starts_with(TCP) {
        if ipv4 {
            Ok(NetworkType::Tcp4)
        } else {
            Ok(NetworkType::Tcp6)
        }
    } else {
        Err(ERR_DETERMINE_NETWORK_TYPE.to_owned())
    }
}
