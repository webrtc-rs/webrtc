use ice::candidate::CandidateType;
use serde::{Deserialize, Serialize};
use std::fmt;

/// ICECandidateType represents the type of the ICE candidate used.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RTCIceCandidateType {
    Unspecified,

    /// ICECandidateTypeHost indicates that the candidate is of Host type as
    /// described in <https://tools.ietf.org/html/rfc8445#section-5.1.1.1>. A
    /// candidate obtained by binding to a specific port from an IP address on
    /// the host. This includes IP addresses on physical interfaces and logical
    /// ones, such as ones obtained through VPNs.
    #[serde(rename = "host")]
    Host,

    /// ICECandidateTypeSrflx indicates the the candidate is of Server
    /// Reflexive type as described
    /// <https://tools.ietf.org/html/rfc8445#section-5.1.1.2>. A candidate type
    /// whose IP address and port are a binding allocated by a NAT for an ICE
    /// agent after it sends a packet through the NAT to a server, such as a
    /// STUN server.
    #[serde(rename = "srflx")]
    Srflx,

    /// ICECandidateTypePrflx indicates that the candidate is of Peer
    /// Reflexive type. A candidate type whose IP address and port are a binding
    /// allocated by a NAT for an ICE agent after it sends a packet through the
    /// NAT to its peer.
    #[serde(rename = "prflx")]
    Prflx,

    /// ICECandidateTypeRelay indicates the the candidate is of Relay type as
    /// described in <https://tools.ietf.org/html/rfc8445#section-5.1.1.2>. A
    /// candidate type obtained from a relay server, such as a TURN server.
    #[serde(rename = "relay")]
    Relay,
}

impl Default for RTCIceCandidateType {
    fn default() -> Self {
        RTCIceCandidateType::Unspecified
    }
}

const ICE_CANDIDATE_TYPE_HOST_STR: &str = "host";
const ICE_CANDIDATE_TYPE_SRFLX_STR: &str = "srflx";
const ICE_CANDIDATE_TYPE_PRFLX_STR: &str = "prflx";
const ICE_CANDIDATE_TYPE_RELAY_STR: &str = "relay";

///  takes a string and converts it into ICECandidateType
impl From<&str> for RTCIceCandidateType {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_CANDIDATE_TYPE_HOST_STR => RTCIceCandidateType::Host,
            ICE_CANDIDATE_TYPE_SRFLX_STR => RTCIceCandidateType::Srflx,
            ICE_CANDIDATE_TYPE_PRFLX_STR => RTCIceCandidateType::Prflx,
            ICE_CANDIDATE_TYPE_RELAY_STR => RTCIceCandidateType::Relay,
            _ => RTCIceCandidateType::Unspecified,
        }
    }
}

impl From<CandidateType> for RTCIceCandidateType {
    fn from(candidate_type: CandidateType) -> Self {
        match candidate_type {
            CandidateType::Host => RTCIceCandidateType::Host,
            CandidateType::ServerReflexive => RTCIceCandidateType::Srflx,
            CandidateType::PeerReflexive => RTCIceCandidateType::Prflx,
            CandidateType::Relay => RTCIceCandidateType::Relay,
            _ => RTCIceCandidateType::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceCandidateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceCandidateType::Host => write!(f, "{ICE_CANDIDATE_TYPE_HOST_STR}"),
            RTCIceCandidateType::Srflx => write!(f, "{ICE_CANDIDATE_TYPE_SRFLX_STR}"),
            RTCIceCandidateType::Prflx => write!(f, "{ICE_CANDIDATE_TYPE_PRFLX_STR}"),
            RTCIceCandidateType::Relay => write!(f, "{ICE_CANDIDATE_TYPE_RELAY_STR}"),
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ice_candidate_type() {
        let tests = vec![
            ("Unspecified", RTCIceCandidateType::Unspecified),
            ("host", RTCIceCandidateType::Host),
            ("srflx", RTCIceCandidateType::Srflx),
            ("prflx", RTCIceCandidateType::Prflx),
            ("relay", RTCIceCandidateType::Relay),
        ];

        for (type_string, expected_type) in tests {
            let actual = RTCIceCandidateType::from(type_string);
            assert_eq!(actual, expected_type);
        }
    }

    #[test]
    fn test_ice_candidate_type_string() {
        let tests = vec![
            (RTCIceCandidateType::Unspecified, "Unspecified"),
            (RTCIceCandidateType::Host, "host"),
            (RTCIceCandidateType::Srflx, "srflx"),
            (RTCIceCandidateType::Prflx, "prflx"),
            (RTCIceCandidateType::Relay, "relay"),
        ];

        for (ctype, expected_string) in tests {
            assert_eq!(ctype.to_string(), expected_string);
        }
    }
}
