use std::fmt;

/// RTCPMuxPolicy affects what ICE candidates are gathered to support
/// non-multiplexed RTCP.
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum RTCPMuxPolicy {
    Unspecified = 0,

    /// RTCPMuxPolicyNegotiate indicates to gather ICE candidates for both
    /// RTP and RTCP candidates. If the remote-endpoint is capable of
    /// multiplexing RTCP, multiplex RTCP on the RTP candidates. If it is not,
    /// use both the RTP and RTCP candidates separately.
    Negotiate = 1,

    /// RTCPMuxPolicyRequire indicates to gather ICE candidates only for
    /// RTP and multiplex RTCP on the RTP candidates. If the remote endpoint is
    /// not capable of rtcp-mux, session negotiation will fail.
    Require = 2,
}

impl Default for RTCPMuxPolicy {
    fn default() -> Self {
        RTCPMuxPolicy::Negotiate
    }
}

const RTCP_MUX_POLICY_NEGOTIATE_STR: &str = "Negotiate";
const RTCP_MUX_POLICY_REQUIRE_STR: &str = "Require";

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
            RTCPMuxPolicy::Unspecified => "Unspecified",
        };
        write!(f, "{}", s)
    }
}

/*
// UnmarshalJSON parses the JSON-encoded data and stores the result
func (t *RTCPMuxPolicy) UnmarshalJSON(b []byte) error {
    var val string
    if err := json.Unmarshal(b, &val); err != nil {
        return err
    }

    *t = newRTCPMuxPolicy(val)
    return nil
}

// MarshalJSON returns the JSON encoding
func (t RTCPMuxPolicy) MarshalJSON() ([]byte, error) {
    return json.Marshal(t.String())
}
*/

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_rtcp_mux_policy() {
        let tests = vec![
            ("Unspecified", RTCPMuxPolicy::Unspecified),
            ("Negotiate", RTCPMuxPolicy::Negotiate),
            ("Require", RTCPMuxPolicy::Require),
        ];

        for (policy_string, expected_policy) in tests {
            assert_eq!(expected_policy, RTCPMuxPolicy::from(policy_string));
        }
    }

    #[test]
    fn test_rtcp_mux_policy_string() {
        let tests = vec![
            (RTCPMuxPolicy::Unspecified, "Unspecified"),
            (RTCPMuxPolicy::Negotiate, "Negotiate"),
            (RTCPMuxPolicy::Require, "Require"),
        ];

        for (policy, expected_string) in tests {
            assert_eq!(expected_string, policy.to_string());
        }
    }
}
