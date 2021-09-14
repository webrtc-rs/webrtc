use serde::{Deserialize, Serialize};
use std::fmt;

/// RTCPMuxPolicy affects what ICE candidates are gathered to support
/// non-multiplexed RTCP.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum RTCPMuxPolicy {
    Unspecified = 0,

    /// RTCPMuxPolicyNegotiate indicates to gather ICE candidates for both
    /// RTP and RTCP candidates. If the remote-endpoint is capable of
    /// multiplexing RTCP, multiplex RTCP on the RTP candidates. If it is not,
    /// use both the RTP and RTCP candidates separately.
    #[serde(rename = "negotiate")]
    Negotiate = 1,

    /// RTCPMuxPolicyRequire indicates to gather ICE candidates only for
    /// RTP and multiplex RTCP on the RTP candidates. If the remote endpoint is
    /// not capable of rtcp-mux, session negotiation will fail.
    #[serde(rename = "require")]
    Require = 2,
}

impl Default for RTCPMuxPolicy {
    fn default() -> Self {
        RTCPMuxPolicy::Negotiate
    }
}

const RTCP_MUX_POLICY_NEGOTIATE_STR: &str = "negotiate";
const RTCP_MUX_POLICY_REQUIRE_STR: &str = "require";

impl From<&str> for RTCPMuxPolicy {
    fn from(raw: &str) -> Self {
        match raw {
            RTCP_MUX_POLICY_NEGOTIATE_STR => RTCPMuxPolicy::Negotiate,
            RTCP_MUX_POLICY_REQUIRE_STR => RTCPMuxPolicy::Require,
            _ => RTCPMuxPolicy::Unspecified,
        }
    }
}

impl fmt::Display for RTCPMuxPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCPMuxPolicy::Negotiate => RTCP_MUX_POLICY_NEGOTIATE_STR,
            RTCPMuxPolicy::Require => RTCP_MUX_POLICY_REQUIRE_STR,
            RTCPMuxPolicy::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_rtcp_mux_policy() {
        let tests = vec![
            ("Unspecified", RTCPMuxPolicy::Unspecified),
            ("negotiate", RTCPMuxPolicy::Negotiate),
            ("require", RTCPMuxPolicy::Require),
        ];

        for (policy_string, expected_policy) in tests {
            assert_eq!(expected_policy, RTCPMuxPolicy::from(policy_string));
        }
    }

    #[test]
    fn test_rtcp_mux_policy_string() {
        let tests = vec![
            (RTCPMuxPolicy::Unspecified, "Unspecified"),
            (RTCPMuxPolicy::Negotiate, "negotiate"),
            (RTCPMuxPolicy::Require, "require"),
        ];

        for (policy, expected_string) in tests {
            assert_eq!(expected_string, policy.to_string());
        }
    }
}
