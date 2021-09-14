use serde::{Deserialize, Serialize};
use std::fmt;

/// ICEProtocol indicates the transport protocol type that is used in the
/// ice.URL structure.
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum ICEProtocol {
    Unspecified,

    /// UDP indicates the URL uses a UDP transport.
    #[serde(rename = "udp")]
    Udp,

    /// TCP indicates the URL uses a TCP transport.
    #[serde(rename = "tcp")]
    Tcp,
}

impl Default for ICEProtocol {
    fn default() -> Self {
        ICEProtocol::Unspecified
    }
}

const ICE_PROTOCOL_UDP_STR: &str = "udp";
const ICE_PROTOCOL_TCP_STR: &str = "tcp";

/// takes a string and converts it to ICEProtocol
impl From<&str> for ICEProtocol {
    fn from(raw: &str) -> Self {
        if raw.to_uppercase() == ICE_PROTOCOL_UDP_STR.to_uppercase() {
            ICEProtocol::Udp
        } else if raw.to_uppercase() == ICE_PROTOCOL_TCP_STR.to_uppercase() {
            ICEProtocol::Tcp
        } else {
            ICEProtocol::Unspecified
        }
    }
}

impl fmt::Display for ICEProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ICEProtocol::Udp => write!(f, "{}", ICE_PROTOCOL_UDP_STR),
            ICEProtocol::Tcp => write!(f, "{}", ICE_PROTOCOL_TCP_STR),
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_ice_protocol() {
        let tests = vec![
            ("Unspecified", ICEProtocol::Unspecified),
            ("udp", ICEProtocol::Udp),
            ("tcp", ICEProtocol::Tcp),
            ("UDP", ICEProtocol::Udp),
            ("TCP", ICEProtocol::Tcp),
        ];

        for (proto_string, expected_proto) in tests {
            let actual = ICEProtocol::from(proto_string);
            assert_eq!(expected_proto, actual);
        }
    }

    #[test]
    fn test_ice_protocol_string() {
        let tests = vec![
            (ICEProtocol::Unspecified, "Unspecified"),
            (ICEProtocol::Udp, "udp"),
            (ICEProtocol::Tcp, "tcp"),
        ];

        for (proto, expected_string) in tests {
            assert_eq!(expected_string, proto.to_string());
        }
    }
}
