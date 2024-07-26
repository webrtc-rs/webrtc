use std::fmt;

use serde::{Deserialize, Serialize};

/// SDPType describes the type of an SessionDescription.
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCSessionDescription/type
/// [W3C]: https://w3c.github.io/webrtc-pc/#dom-rtcsessiondescription-type
#[derive(Default, Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum RTCSdpType {
    #[default]
    Unspecified = 0,

    /// indicates that a description MUST be treated as an SDP offer.
    #[serde(rename = "offer")]
    Offer,

    /// indicates that a description MUST be treated as an
    /// SDP answer, but not a final answer. A description used as an SDP
    /// pranswer may be applied as a response to an SDP offer, or an update to
    /// a previously sent SDP pranswer.
    #[serde(rename = "pranswer")]
    Pranswer,

    /// indicates that a description MUST be treated as an SDP
    /// final answer, and the offer-answer exchange MUST be considered complete.
    /// A description used as an SDP answer may be applied as a response to an
    /// SDP offer or as an update to a previously sent SDP pranswer.    
    #[serde(rename = "answer")]
    Answer,

    /// indicates that a description MUST be treated as
    /// canceling the current SDP negotiation and moving the SDP offer and
    /// answer back to what it was in the previous stable state. Note the
    /// local or remote SDP descriptions in the previous stable state could be
    /// null if there has not yet been a successful offer-answer negotiation.
    #[serde(rename = "rollback")]
    Rollback,
}

const SDP_TYPE_OFFER_STR: &str = "offer";
const SDP_TYPE_PRANSWER_STR: &str = "pranswer";
const SDP_TYPE_ANSWER_STR: &str = "answer";
const SDP_TYPE_ROLLBACK_STR: &str = "rollback";

/// creates an SDPType from a string
impl From<&str> for RTCSdpType {
    fn from(raw: &str) -> Self {
        match raw {
            SDP_TYPE_OFFER_STR => RTCSdpType::Offer,
            SDP_TYPE_PRANSWER_STR => RTCSdpType::Pranswer,
            SDP_TYPE_ANSWER_STR => RTCSdpType::Answer,
            SDP_TYPE_ROLLBACK_STR => RTCSdpType::Rollback,
            _ => RTCSdpType::Unspecified,
        }
    }
}

impl fmt::Display for RTCSdpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCSdpType::Offer => write!(f, "{SDP_TYPE_OFFER_STR}"),
            RTCSdpType::Pranswer => write!(f, "{SDP_TYPE_PRANSWER_STR}"),
            RTCSdpType::Answer => write!(f, "{SDP_TYPE_ANSWER_STR}"),
            RTCSdpType::Rollback => write!(f, "{SDP_TYPE_ROLLBACK_STR}"),
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_sdp_type() {
        let tests = vec![
            ("Unspecified", RTCSdpType::Unspecified),
            ("offer", RTCSdpType::Offer),
            ("pranswer", RTCSdpType::Pranswer),
            ("answer", RTCSdpType::Answer),
            ("rollback", RTCSdpType::Rollback),
        ];

        for (sdp_type_string, expected_sdp_type) in tests {
            assert_eq!(RTCSdpType::from(sdp_type_string), expected_sdp_type);
        }
    }

    #[test]
    fn test_sdp_type_string() {
        let tests = vec![
            (RTCSdpType::Unspecified, "Unspecified"),
            (RTCSdpType::Offer, "offer"),
            (RTCSdpType::Pranswer, "pranswer"),
            (RTCSdpType::Answer, "answer"),
            (RTCSdpType::Rollback, "rollback"),
        ];

        for (sdp_type, expected_string) in tests {
            assert_eq!(sdp_type.to_string(), expected_string);
        }
    }
}
