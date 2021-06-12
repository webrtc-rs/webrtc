use std::fmt;

/// ICEGathererState represents the current state of the ICE gatherer.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ICEGathererState {
    Unspecified,

    /// ICEGathererStateNew indicates object has been created but
    /// gather() has not been called.
    New,

    /// ICEGathererStateGathering indicates gather() has been called,
    /// and the ICEGatherer is in the process of gathering candidates.
    Gathering,

    /// ICEGathererStateComplete indicates the ICEGatherer has completed gathering.
    Complete,

    /// ICEGathererStateClosed indicates the closed state can only be entered
    /// when the ICEGatherer has been closed intentionally by calling close().
    Closed,
}

impl Default for ICEGathererState {
    fn default() -> Self {
        ICEGathererState::Unspecified
    }
}

const ICE_GATHERED_STATE_NEW_STR: &str = "New";
const ICE_GATHERED_STATE_GATHERING_STR: &str = "Gathering";
const ICE_GATHERED_STATE_COMPLETE_STR: &str = "Complete";
const ICE_GATHERED_STATE_CLOSED_STR: &str = "Closed";

impl From<&str> for ICEGathererState {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_GATHERED_STATE_NEW_STR => ICEGathererState::New,
            ICE_GATHERED_STATE_GATHERING_STR => ICEGathererState::Gathering,
            ICE_GATHERED_STATE_COMPLETE_STR => ICEGathererState::Complete,
            ICE_GATHERED_STATE_CLOSED_STR => ICEGathererState::Closed,
            _ => ICEGathererState::Unspecified,
        }
    }
}

impl From<u8> for ICEGathererState {
    fn from(v: u8) -> Self {
        match v {
            1 => ICEGathererState::New,
            2 => ICEGathererState::Gathering,
            3 => ICEGathererState::Complete,
            4 => ICEGathererState::Closed,
            _ => ICEGathererState::Unspecified,
        }
    }
}

impl fmt::Display for ICEGathererState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ICEGathererState::New => write!(f, "{}", ICE_GATHERED_STATE_NEW_STR),
            ICEGathererState::Gathering => write!(f, "{}", ICE_GATHERED_STATE_GATHERING_STR),
            ICEGathererState::Complete => {
                write!(f, "{}", ICE_GATHERED_STATE_COMPLETE_STR)
            }
            ICEGathererState::Closed => {
                write!(f, "{}", ICE_GATHERED_STATE_CLOSED_STR)
            }
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ice_gatherer_state_string() {
        let tests = vec![
            (ICEGathererState::Unspecified, "Unspecified"),
            (ICEGathererState::New, "New"),
            (ICEGathererState::Gathering, "Gathering"),
            (ICEGathererState::Complete, "Complete"),
            (ICEGathererState::Closed, "Closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(expected_string, state.to_string());
        }
    }
}
