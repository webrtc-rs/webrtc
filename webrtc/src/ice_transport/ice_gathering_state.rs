use std::fmt;

/// ICEGatheringState describes the state of the candidate gathering process.
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCPeerConnection/iceGatheringState
/// [W3C]: https://w3c.github.io/webrtc-pc/#dom-peerconnection-ice-gathering-state
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCIceGatheringState {
    #[default]
    Unspecified,

    /// ICEGatheringStateNew indicates that any of the ICETransports are
    /// in the "new" gathering state and none of the transports are in the
    /// "gathering" state, or there are no transports.
    New,

    /// ICEGatheringStateGathering indicates that any of the ICETransports
    /// are in the "gathering" state.
    Gathering,

    /// ICEGatheringStateComplete indicates that at least one ICETransport
    /// exists, and all ICETransports are in the "completed" gathering state.
    Complete,
}

const ICE_GATHERING_STATE_NEW_STR: &str = "new";
const ICE_GATHERING_STATE_GATHERING_STR: &str = "gathering";
const ICE_GATHERING_STATE_COMPLETE_STR: &str = "complete";

/// takes a string and converts it to ICEGatheringState
impl From<&str> for RTCIceGatheringState {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_GATHERING_STATE_NEW_STR => RTCIceGatheringState::New,
            ICE_GATHERING_STATE_GATHERING_STR => RTCIceGatheringState::Gathering,
            ICE_GATHERING_STATE_COMPLETE_STR => RTCIceGatheringState::Complete,
            _ => RTCIceGatheringState::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceGatheringState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceGatheringState::New => write!(f, "{ICE_GATHERING_STATE_NEW_STR}"),
            RTCIceGatheringState::Gathering => write!(f, "{ICE_GATHERING_STATE_GATHERING_STR}"),
            RTCIceGatheringState::Complete => {
                write!(f, "{ICE_GATHERING_STATE_COMPLETE_STR}")
            }
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_ice_gathering_state() {
        let tests = vec![
            ("Unspecified", RTCIceGatheringState::Unspecified),
            ("new", RTCIceGatheringState::New),
            ("gathering", RTCIceGatheringState::Gathering),
            ("complete", RTCIceGatheringState::Complete),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(RTCIceGatheringState::from(state_string), expected_state);
        }
    }

    #[test]
    fn test_ice_gathering_state_string() {
        let tests = vec![
            (RTCIceGatheringState::Unspecified, "Unspecified"),
            (RTCIceGatheringState::New, "new"),
            (RTCIceGatheringState::Gathering, "gathering"),
            (RTCIceGatheringState::Complete, "complete"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(state.to_string(), expected_string);
        }
    }
}
