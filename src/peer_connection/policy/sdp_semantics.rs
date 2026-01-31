use std::fmt;

use serde::{Deserialize, Serialize};

/// SDPSemantics determines which style of SDP offers and answers
/// can be used.
///
/// This is unused, we only support UnifiedPlan.
#[derive(Default, Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum RTCSdpSemantics {
    Unspecified = 0,

    /// UnifiedPlan uses unified-plan offers and answers
    /// (the default in Chrome since M72)
    /// <https://tools.ietf.org/html/draft-roach-mmusic-unified-plan-00>
    #[serde(rename = "unified-plan")]
    #[default]
    UnifiedPlan = 1,

    /// PlanB uses plan-b offers and answers
    /// NB: This format should be considered deprecated
    /// <https://tools.ietf.org/html/draft-uberti-rtcweb-plan-00>
    #[serde(rename = "plan-b")]
    PlanB = 2,

    /// UnifiedPlanWithFallback prefers unified-plan
    /// offers and answers, but will respond to a plan-b offer
    /// with a plan-b answer
    #[serde(rename = "unified-plan-with-fallback")]
    UnifiedPlanWithFallback = 3,
}

const SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK: &str = "unified-plan-with-fallback";
const SDP_SEMANTICS_UNIFIED_PLAN: &str = "unified-plan";
const SDP_SEMANTICS_PLAN_B: &str = "plan-b";

impl From<&str> for RTCSdpSemantics {
    fn from(raw: &str) -> Self {
        match raw {
            SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK => RTCSdpSemantics::UnifiedPlanWithFallback,
            SDP_SEMANTICS_UNIFIED_PLAN => RTCSdpSemantics::UnifiedPlan,
            SDP_SEMANTICS_PLAN_B => RTCSdpSemantics::PlanB,
            _ => RTCSdpSemantics::Unspecified,
        }
    }
}

impl fmt::Display for RTCSdpSemantics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCSdpSemantics::UnifiedPlanWithFallback => SDP_SEMANTICS_UNIFIED_PLAN_WITH_FALLBACK,
            RTCSdpSemantics::UnifiedPlan => SDP_SEMANTICS_UNIFIED_PLAN,
            RTCSdpSemantics::PlanB => SDP_SEMANTICS_PLAN_B,
            RTCSdpSemantics::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use sdp::description::media::MediaDescription;
    use sdp::description::session::{SessionDescription, ATTR_KEY_SSRC};

    use super::*;

    #[test]
    fn test_sdp_semantics_string() {
        let tests = vec![
            (RTCSdpSemantics::Unspecified, "Unspecified"),
            (
                RTCSdpSemantics::UnifiedPlanWithFallback,
                "unified-plan-with-fallback",
            ),
            (RTCSdpSemantics::PlanB, "plan-b"),
            (RTCSdpSemantics::UnifiedPlan, "unified-plan"),
        ];

        for (value, expected_string) in tests {
            assert_eq!(value.to_string(), expected_string);
        }
    }

    // The following tests are for non-standard SDP semantics
    // (i.e. not unified-unified)
    fn get_md_names(sdp: &SessionDescription) -> Vec<String> {
        sdp.media_descriptions
            .iter()
            .map(|md| md.media_name.media.clone())
            .collect()
    }

    fn extract_ssrc_list(md: &MediaDescription) -> Vec<String> {
        let mut ssrcs = HashSet::new();
        for attr in &md.attributes {
            if attr.key == ATTR_KEY_SSRC {
                if let Some(value) = &attr.value {
                    let fields: Vec<&str> = value.split_whitespace().collect();
                    if let Some(ssrc) = fields.first() {
                        ssrcs.insert(*ssrc);
                    }
                }
            }
        }
        ssrcs
            .into_iter()
            .map(|ssrc| ssrc.to_owned())
            .collect::<Vec<String>>()
    }
}
