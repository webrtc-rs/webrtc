use serde::{Deserialize, Serialize};
use std::fmt;

/// ICETransportPolicy defines the ICE candidate policy surface the
/// permitted candidates. Only these candidates are used for connectivity checks.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum ICETransportPolicy {
    Unspecified = 0,

    /// ICETransportPolicyAll indicates any type of candidate is used.
    #[serde(rename = "all")]
    All = 1,

    /// ICETransportPolicyRelay indicates only media relay candidates such
    /// as candidates passing through a TURN server are used.
    #[serde(rename = "relay")]
    Relay = 2,
}

impl Default for ICETransportPolicy {
    fn default() -> Self {
        ICETransportPolicy::Unspecified
    }
}

/// ICEGatherPolicy is the ORTC equivalent of ICETransportPolicy
pub type ICEGatherPolicy = ICETransportPolicy;

const ICE_TRANSPORT_POLICY_RELAY_STR: &str = "relay";
const ICE_TRANSPORT_POLICY_ALL_STR: &str = "all";

/// takes a string and converts it to ICETransportPolicy
impl From<&str> for ICETransportPolicy {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_TRANSPORT_POLICY_RELAY_STR => ICETransportPolicy::Relay,
            ICE_TRANSPORT_POLICY_ALL_STR => ICETransportPolicy::All,
            _ => ICETransportPolicy::Unspecified,
        }
    }
}

impl fmt::Display for ICETransportPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ICETransportPolicy::Relay => ICE_TRANSPORT_POLICY_RELAY_STR,
            ICETransportPolicy::All => ICE_TRANSPORT_POLICY_ALL_STR,
            ICETransportPolicy::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_ice_transport_policy() {
        let tests = vec![
            ("relay", ICETransportPolicy::Relay),
            ("all", ICETransportPolicy::All),
        ];

        for (policy_string, expected_policy) in tests {
            assert_eq!(expected_policy, ICETransportPolicy::from(policy_string));
        }
    }

    #[test]
    fn test_ice_transport_policy_string() {
        let tests = vec![
            (ICETransportPolicy::Relay, "relay"),
            (ICETransportPolicy::All, "all"),
        ];

        for (policy, expected_string) in tests {
            assert_eq!(expected_string, policy.to_string());
        }
    }
}
