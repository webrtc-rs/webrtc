use std::fmt;

/// ICETransportPolicy defines the ICE candidate policy surface the
/// permitted candidates. Only these candidates are used for connectivity checks.
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ICETransportPolicy {
    Unspecified = 0,

    /// ICETransportPolicyAll indicates any type of candidate is used.
    All = 1,

    /// ICETransportPolicyRelay indicates only media relay candidates such
    /// as candidates passing through a TURN server are used.
    Relay = 2,
}

impl Default for ICETransportPolicy {
    fn default() -> Self {
        ICETransportPolicy::Unspecified
    }
}

/// ICEGatherPolicy is the ORTC equivalent of ICETransportPolicy
pub type ICEGatherPolicy = ICETransportPolicy;

const ICE_TRANSPORT_POLICY_RELAY_STR: &str = "Relay";
const ICE_TRANSPORT_POLICY_ALL_STR: &str = "All";

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

/*
// UnmarshalJSON parses the JSON-encoded data and stores the result
func (t *ICETransportPolicy) UnmarshalJSON(b []byte) error {
    var val string
    if err := json.Unmarshal(b, &val); err != nil {
        return err
    }
    *t = NewICETransportPolicy(val)
    return nil
}

// MarshalJSON returns the JSON encoding
func (t ICETransportPolicy) MarshalJSON() ([]byte, error) {
    return json.Marshal(t.String())
}
*/
