use serde::{Deserialize, Serialize};
use std::fmt;

/// ICEProtocol indicates the transport protocol type that is used in the
/// ice.URL structure.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RTCIceProtocol {
    Unspecified,

    /// UDP indicates the URL uses a UDP transport.
    #[serde(rename = "udp")]
    Udp,

    /// TCP indicates the URL uses a TCP transport.
    #[serde(rename = "tcp")]
    Tcp,
}

impl Default for RTCIceProtocol {
    fn default() -> Self {
        RTCIceProtocol::Unspecified
    }
}

const ICE_PROTOCOL_UDP_STR: &str = "udp";
const ICE_PROTOCOL_TCP_STR: &str = "tcp";

/// takes a string and converts it to ICEProtocol
impl From<&str> for RTCIceProtocol {
    fn from(raw: &str) -> Self {
        if raw.to_uppercase() == ICE_PROTOCOL_UDP_STR.to_uppercase() {
            RTCIceProtocol::Udp
        } else if raw.to_uppercase() == ICE_PROTOCOL_TCP_STR.to_uppercase() {
            RTCIceProtocol::Tcp
        } else {
            RTCIceProtocol::Unspecified
        }
    }
}

impl fmt::Display for RTCIceProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceProtocol::Udp => write!(f, "{ICE_PROTOCOL_UDP_STR}"),
            RTCIceProtocol::Tcp => write!(f, "{ICE_PROTOCOL_TCP_STR}"),
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
            ("Unspecified", RTCIceProtocol::Unspecified),
            ("udp", RTCIceProtocol::Udp),
            ("tcp", RTCIceProtocol::Tcp),
            ("UDP", RTCIceProtocol::Udp),
            ("TCP", RTCIceProtocol::Tcp),
        ];

        for (proto_string, expected_proto) in tests {
            let actual = RTCIceProtocol::from(proto_string);
            assert_eq!(actual, expected_proto);
        }
    }

    #[test]
    fn test_ice_protocol_string() {
        let tests = vec![
            (RTCIceProtocol::Unspecified, "Unspecified"),
            (RTCIceProtocol::Udp, "udp"),
            (RTCIceProtocol::Tcp, "tcp"),
        ];

        for (proto, expected_string) in tests {
            assert_eq!(proto.to_string(), expected_string);
        }
    }
}
