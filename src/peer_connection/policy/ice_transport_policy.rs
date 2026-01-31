use std::fmt;

use serde::{Deserialize, Serialize};

/// ICETransportPolicy defines the ICE candidate policy surface the
/// permitted candidates. Only these candidates are used for connectivity checks.
///
/// ## Specifications
///
/// * [W3C]
///
/// [W3C]: https://w3c.github.io/webrtc-pc/#rtcicetransportpolicy-enum
#[derive(Default, Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum RTCIceTransportPolicy {
    #[default]
    Unspecified = 0,

    /// ICETransportPolicyAll indicates any type of candidate is used.
    #[serde(rename = "all")]
    All = 1,

    /// ICETransportPolicyRelay indicates only media relay candidates such
    /// as candidates passing through a TURN server are used.
    #[serde(rename = "relay")]
    Relay = 2,
}

/// ICEGatherPolicy is the ORTC equivalent of ICETransportPolicy
pub type ICEGatherPolicy = RTCIceTransportPolicy;

const ICE_TRANSPORT_POLICY_RELAY_STR: &str = "relay";
const ICE_TRANSPORT_POLICY_ALL_STR: &str = "all";

/// takes a string and converts it to ICETransportPolicy
impl From<&str> for RTCIceTransportPolicy {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_TRANSPORT_POLICY_RELAY_STR => RTCIceTransportPolicy::Relay,
            ICE_TRANSPORT_POLICY_ALL_STR => RTCIceTransportPolicy::All,
            _ => RTCIceTransportPolicy::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceTransportPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCIceTransportPolicy::Relay => ICE_TRANSPORT_POLICY_RELAY_STR,
            RTCIceTransportPolicy::All => ICE_TRANSPORT_POLICY_ALL_STR,
            RTCIceTransportPolicy::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_ice_transport_policy() {
        let tests = vec![
            ("relay", RTCIceTransportPolicy::Relay),
            ("all", RTCIceTransportPolicy::All),
        ];

        for (policy_string, expected_policy) in tests {
            assert_eq!(RTCIceTransportPolicy::from(policy_string), expected_policy);
        }
    }

    #[test]
    fn test_ice_transport_policy_string() {
        let tests = vec![
            (RTCIceTransportPolicy::Relay, "relay"),
            (RTCIceTransportPolicy::All, "all"),
        ];

        for (policy, expected_string) in tests {
            assert_eq!(policy.to_string(), expected_string);
        }
    }
}
