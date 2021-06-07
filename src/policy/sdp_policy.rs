use std::fmt;

/// SdpPolicy determines which style of SDP offers and answers
/// can be used
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SdpPolicy {
    Unspecified = 0,

    /// UnifiedPlan uses unified-plan offers and answers
    /// (the default in Chrome since M72)
    /// https://tools.ietf.org/html/draft-roach-mmusic-unified-plan-00
    UnifiedPlan = 1,

    /// PlanB uses plan-b offers and answers
    /// NB: This format should be considered deprecated
    /// https://tools.ietf.org/html/draft-uberti-rtcweb-plan-00
    PlanB = 2,

    /// UnifiedPlanWithFallback prefers unified-plan
    /// offers and answers, but will respond to a plan-b offer
    /// with a plan-b answer
    UnifiedPlanWithFallback = 3,
}

impl Default for SdpPolicy {
    fn default() -> Self {
        SdpPolicy::Unspecified
    }
}

const SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK: &str = "UnifiedPlanWithFallback";
const SDP_SEMANTICS_UNIFIED_PLAN: &str = "UnifiedPlan";
const SDP_SEMANTICS_PLAN_B: &str = "PlanB";

impl From<&str> for SdpPolicy {
    fn from(raw: &str) -> Self {
        match raw {
            SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK => SdpPolicy::UnifiedPlanWithFallback,
            SDP_SEMANTICS_UNIFIED_PLAN => SdpPolicy::UnifiedPlan,
            SDP_SEMANTICS_PLAN_B => SdpPolicy::PlanB,
            _ => SdpPolicy::Unspecified,
        }
    }
}

impl fmt::Display for SdpPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            SdpPolicy::UnifiedPlanWithFallback => SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK,
            SdpPolicy::UnifiedPlan => SDP_SEMANTICS_UNIFIED_PLAN,
            SdpPolicy::PlanB => SDP_SEMANTICS_PLAN_B,
            SdpPolicy::Unspecified => "Unspecified",
        };
        write!(f, "{}", s)
    }
}

/*
// UnmarshalJSON parses the JSON-encoded data and stores the result
func (s *SdpPolicy) UnmarshalJSON(b []byte) error {
    var val string
    if err := json.Unmarshal(b, &val); err != nil {
        return err
    }

    *s = newSDPSemantics(val)
    return nil
}

// MarshalJSON returns the JSON encoding
func (s SdpPolicy) MarshalJSON() ([]byte, error) {
    return json.Marshal(s.String())
}
*/
