/// ICETransportPolicy defines the ICE candidate policy surface the
/// permitted candidates. Only these candidates are used for connectivity checks.
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ICETransportPolicy {
    /// ICETransportPolicyAll indicates any type of candidate is used.
    All,

    /// ICETransportPolicyRelay indicates only media relay candidates such
    /// as candidates passing through a TURN server are used.
    Relay,
}

impl Default for ICETransportPolicy {
    fn default() -> Self {
        ICETransportPolicy::All
    }
}

//// ICEGatherPolicy is the ORTC equivalent of ICETransportPolicy
pub type ICEGatherPolicy = ICETransportPolicy;
/*
// This is done this way because of a linter.
const (
    iceTransportPolicyRelayStr = "relay"
    iceTransportPolicyAllStr   = "all"
)

// NewICETransportPolicy takes a string and converts it to ICETransportPolicy
func NewICETransportPolicy(raw string) ICETransportPolicy {
    switch raw {
    case iceTransportPolicyRelayStr:
        return ICETransportPolicyRelay
    case iceTransportPolicyAllStr:
        return ICETransportPolicyAll
    default:
        return ICETransportPolicy(Unknown)
    }
}

func (t ICETransportPolicy) String() string {
    switch t {
    case ICETransportPolicyRelay:
        return iceTransportPolicyRelayStr
    case ICETransportPolicyAll:
        return iceTransportPolicyAllStr
    default:
        return ErrUnknownType.Error()
    }
}

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
