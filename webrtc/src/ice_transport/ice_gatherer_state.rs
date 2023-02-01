use std::fmt;

/// ICEGathererState represents the current state of the ICE gatherer.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCIceGathererState {
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

impl Default for RTCIceGathererState {
    fn default() -> Self {
        RTCIceGathererState::Unspecified
    }
}

const ICE_GATHERED_STATE_NEW_STR: &str = "new";
const ICE_GATHERED_STATE_GATHERING_STR: &str = "gathering";
const ICE_GATHERED_STATE_COMPLETE_STR: &str = "complete";
const ICE_GATHERED_STATE_CLOSED_STR: &str = "closed";

impl From<&str> for RTCIceGathererState {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_GATHERED_STATE_NEW_STR => RTCIceGathererState::New,
            ICE_GATHERED_STATE_GATHERING_STR => RTCIceGathererState::Gathering,
            ICE_GATHERED_STATE_COMPLETE_STR => RTCIceGathererState::Complete,
            ICE_GATHERED_STATE_CLOSED_STR => RTCIceGathererState::Closed,
            _ => RTCIceGathererState::Unspecified,
        }
    }
}

impl From<u8> for RTCIceGathererState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCIceGathererState::New,
            2 => RTCIceGathererState::Gathering,
            3 => RTCIceGathererState::Complete,
            4 => RTCIceGathererState::Closed,
            _ => RTCIceGathererState::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceGathererState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceGathererState::New => write!(f, "{ICE_GATHERED_STATE_NEW_STR}"),
            RTCIceGathererState::Gathering => write!(f, "{ICE_GATHERED_STATE_GATHERING_STR}"),
            RTCIceGathererState::Complete => {
                write!(f, "{ICE_GATHERED_STATE_COMPLETE_STR}")
            }
            RTCIceGathererState::Closed => {
                write!(f, "{ICE_GATHERED_STATE_CLOSED_STR}")
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
            (RTCIceGathererState::Unspecified, "Unspecified"),
            (RTCIceGathererState::New, "new"),
            (RTCIceGathererState::Gathering, "gathering"),
            (RTCIceGathererState::Complete, "complete"),
            (RTCIceGathererState::Closed, "closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(state.to_string(), expected_string);
        }
    }
}
