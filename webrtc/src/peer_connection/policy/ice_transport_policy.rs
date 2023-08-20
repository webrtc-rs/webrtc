use std::fmt;

use serde::{Deserialize, Serialize};

/// Defines the policy that will be used to determine the
/// permitted ICE candidates that will be used for connectivity checks.
#[derive(Default, Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum RTCIceTransportPolicy {
    #[default]
    Unspecified = 0,

    /// Indicates that the ICE Agent can use any type of candidate.
    #[serde(rename = "all")]
    All = 1,

    /// Indicates that the ICE Agent must use only media relay candidates,
    /// such as candidates passing through a TURN server.
    ///
    /// This can be used to prevent the remote endpoint from learning the
    /// user's IP addresses via STUN requests, which may be desirable in
    /// certain use cases.
    ///
    /// Keep in mind that media relay candidates can increase latency,
    /// reduce throughput, and consume more battery power than direct
    /// peer-to-peer connections.
    #[serde(rename = "relay")]
    Relay = 2,
}

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
