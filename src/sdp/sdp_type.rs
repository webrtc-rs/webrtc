use serde::{Deserialize, Serialize};
use std::fmt;

/// SDPType describes the type of an SessionDescription.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum SDPType {
    Unspecified = 0,

    /// indicates that a description MUST be treated as an SDP offer.
    Offer,

    /// indicates that a description MUST be treated as an
    /// SDP answer, but not a final answer. A description used as an SDP
    /// pranswer may be applied as a response to an SDP offer, or an update to
    /// a previously sent SDP pranswer.
    Pranswer,

    /// indicates that a description MUST be treated as an SDP
    /// final answer, and the offer-answer exchange MUST be considered complete.
    /// A description used as an SDP answer may be applied as a response to an
    /// SDP offer or as an update to a previously sent SDP pranswer.
    Answer,

    /// indicates that a description MUST be treated as
    /// canceling the current SDP negotiation and moving the SDP offer and
    /// answer back to what it was in the previous stable state. Note the
    /// local or remote SDP descriptions in the previous stable state could be
    /// null if there has not yet been a successful offer-answer negotiation.
    Rollback,
}

impl Default for SDPType {
    fn default() -> Self {
        SDPType::Unspecified
    }
}

const SDP_TYPE_OFFER_STR: &str = "offer";
const SDP_TYPE_PRANSWER_STR: &str = "pranswer";
const SDP_TYPE_ANSWER_STR: &str = "answer";
const SDP_TYPE_ROLLBACK_STR: &str = "rollback";

/// creates an SDPType from a string
impl From<&str> for SDPType {
    fn from(raw: &str) -> Self {
        match raw {
            SDP_TYPE_OFFER_STR => SDPType::Offer,
            SDP_TYPE_PRANSWER_STR => SDPType::Pranswer,
            SDP_TYPE_ANSWER_STR => SDPType::Answer,
            SDP_TYPE_ROLLBACK_STR => SDPType::Rollback,
            _ => SDPType::Unspecified,
        }
    }
}

impl fmt::Display for SDPType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            SDPType::Offer => write!(f, "{}", SDP_TYPE_OFFER_STR),
            SDPType::Pranswer => write!(f, "{}", SDP_TYPE_PRANSWER_STR),
            SDPType::Answer => write!(f, "{}", SDP_TYPE_ANSWER_STR),
            SDPType::Rollback => write!(f, "{}", SDP_TYPE_ROLLBACK_STR),
            _ => write!(f, "unspecified"),
        }
    }
}

/*
// MarshalJSON enables JSON marshaling of a SDPType
func (t SDPType) MarshalJSON() ([]byte, error) {
    return json.Marshal(t.String())
}

// UnmarshalJSON enables JSON unmarshaling of a SDPType
func (t *SDPType) UnmarshalJSON(b []byte) error {
    var s string
    if err := json.Unmarshal(b, &s); err != nil {
        return err
    }
    switch strings.ToLower(s) {
    default:
        return ErrUnknownType
    case "offer":
        *t = SDPTypeOffer
    case "pranswer":
        *t = SDPTypePranswer
    case "answer":
        *t = SDPTypeAnswer
    case "rollback":
        *t = SDPTypeRollback
    }

    return nil
}
*/

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_sdp_type() {
        let tests = vec![
            ("unspecified", SDPType::Unspecified),
            ("offer", SDPType::Offer),
            ("pranswer", SDPType::Pranswer),
            ("answer", SDPType::Answer),
            ("rollback", SDPType::Rollback),
        ];

        for (sdp_type_string, expected_sdp_type) in tests {
            assert_eq!(expected_sdp_type, SDPType::from(sdp_type_string));
        }
    }

    #[test]
    fn test_sdp_type_string() {
        let tests = vec![
            (SDPType::Unspecified, "unspecified"),
            (SDPType::Offer, "offer"),
            (SDPType::Pranswer, "pranswer"),
            (SDPType::Answer, "answer"),
            (SDPType::Rollback, "rollback"),
        ];

        for (sdp_type, expected_string) in tests {
            assert_eq!(expected_string, sdp_type.to_string());
        }
    }
}
