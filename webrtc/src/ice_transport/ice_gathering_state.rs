use std::fmt;

/// Describes the state of the ICE candidate gathering process.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCIceGatheringState {
    #[default]
    Unspecified,

    /// Any of the [`RTCIceTransport`]s are
    /// in the "new" gathering state and none of the transports are in the
    /// "gathering" state, or there are no transports.
    ///
    /// [`RTCIceTransport`]: crate::ice_transport::RTCIceTransport
    New,

    /// Any of the [`RTCIceTransport`]s
    /// are in the "gathering" state.
    ///
    /// [`RTCIceTransport`]: crate::ice_transport::RTCIceTransport
    Gathering,

    /// At least one [`RTCIceTransport`]
    /// exists, and all [`RTCIceTransport`]s are in the "complete" gathering state.
    ///
    /// [`RTCIceTransport`]: crate::ice_transport::RTCIceTransport
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
