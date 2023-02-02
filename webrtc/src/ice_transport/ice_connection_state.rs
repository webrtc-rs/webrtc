use std::fmt;

/// RTCIceConnectionState indicates signaling state of the ICE Connection.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCIceConnectionState {
    Unspecified,

    /// ICEConnectionStateNew indicates that any of the ICETransports are
    /// in the "new" state and none of them are in the "checking", "disconnected"
    /// or "failed" state, or all ICETransports are in the "closed" state, or
    /// there are no transports.
    New,

    /// ICEConnectionStateChecking indicates that any of the ICETransports
    /// are in the "checking" state and none of them are in the "disconnected"
    /// or "failed" state.
    Checking,

    /// ICEConnectionStateConnected indicates that all ICETransports are
    /// in the "connected", "completed" or "closed" state and at least one of
    /// them is in the "connected" state.
    Connected,

    /// ICEConnectionStateCompleted indicates that all ICETransports are
    /// in the "completed" or "closed" state and at least one of them is in the
    /// "completed" state.
    Completed,

    /// ICEConnectionStateDisconnected indicates that any of the
    /// ICETransports are in the "disconnected" state and none of them are
    /// in the "failed" state.
    Disconnected,

    /// ICEConnectionStateFailed indicates that any of the ICETransports
    /// are in the "failed" state.
    Failed,

    /// ICEConnectionStateClosed indicates that the PeerConnection's
    /// isClosed is true.
    Closed,
}

impl Default for RTCIceConnectionState {
    fn default() -> Self {
        RTCIceConnectionState::Unspecified
    }
}

const ICE_CONNECTION_STATE_NEW_STR: &str = "new";
const ICE_CONNECTION_STATE_CHECKING_STR: &str = "checking";
const ICE_CONNECTION_STATE_CONNECTED_STR: &str = "connected";
const ICE_CONNECTION_STATE_COMPLETED_STR: &str = "completed";
const ICE_CONNECTION_STATE_DISCONNECTED_STR: &str = "disconnected";
const ICE_CONNECTION_STATE_FAILED_STR: &str = "failed";
const ICE_CONNECTION_STATE_CLOSED_STR: &str = "closed";

/// takes a string and converts it to iceconnection_state
impl From<&str> for RTCIceConnectionState {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_CONNECTION_STATE_NEW_STR => RTCIceConnectionState::New,
            ICE_CONNECTION_STATE_CHECKING_STR => RTCIceConnectionState::Checking,
            ICE_CONNECTION_STATE_CONNECTED_STR => RTCIceConnectionState::Connected,
            ICE_CONNECTION_STATE_COMPLETED_STR => RTCIceConnectionState::Completed,
            ICE_CONNECTION_STATE_DISCONNECTED_STR => RTCIceConnectionState::Disconnected,
            ICE_CONNECTION_STATE_FAILED_STR => RTCIceConnectionState::Failed,
            ICE_CONNECTION_STATE_CLOSED_STR => RTCIceConnectionState::Closed,
            _ => RTCIceConnectionState::Unspecified,
        }
    }
}

impl From<u8> for RTCIceConnectionState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCIceConnectionState::New,
            2 => RTCIceConnectionState::Checking,
            3 => RTCIceConnectionState::Connected,
            4 => RTCIceConnectionState::Completed,
            5 => RTCIceConnectionState::Disconnected,
            6 => RTCIceConnectionState::Failed,
            7 => RTCIceConnectionState::Closed,
            _ => RTCIceConnectionState::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCIceConnectionState::New => ICE_CONNECTION_STATE_NEW_STR,
            RTCIceConnectionState::Checking => ICE_CONNECTION_STATE_CHECKING_STR,
            RTCIceConnectionState::Connected => ICE_CONNECTION_STATE_CONNECTED_STR,
            RTCIceConnectionState::Completed => ICE_CONNECTION_STATE_COMPLETED_STR,
            RTCIceConnectionState::Disconnected => ICE_CONNECTION_STATE_DISCONNECTED_STR,
            RTCIceConnectionState::Failed => ICE_CONNECTION_STATE_FAILED_STR,
            RTCIceConnectionState::Closed => ICE_CONNECTION_STATE_CLOSED_STR,
            RTCIceConnectionState::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_ice_connection_state() {
        let tests = vec![
            (crate::UNSPECIFIED_STR, RTCIceConnectionState::Unspecified),
            ("new", RTCIceConnectionState::New),
            ("checking", RTCIceConnectionState::Checking),
            ("connected", RTCIceConnectionState::Connected),
            ("completed", RTCIceConnectionState::Completed),
            ("disconnected", RTCIceConnectionState::Disconnected),
            ("failed", RTCIceConnectionState::Failed),
            ("closed", RTCIceConnectionState::Closed),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(
                RTCIceConnectionState::from(state_string),
                expected_state,
                "testCase: {expected_state}",
            );
        }
    }

    #[test]
    fn test_ice_connection_state_string() {
        let tests = vec![
            (RTCIceConnectionState::Unspecified, crate::UNSPECIFIED_STR),
            (RTCIceConnectionState::New, "new"),
            (RTCIceConnectionState::Checking, "checking"),
            (RTCIceConnectionState::Connected, "connected"),
            (RTCIceConnectionState::Completed, "completed"),
            (RTCIceConnectionState::Disconnected, "disconnected"),
            (RTCIceConnectionState::Failed, "failed"),
            (RTCIceConnectionState::Closed, "closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(state.to_string(), expected_string)
        }
    }
}
