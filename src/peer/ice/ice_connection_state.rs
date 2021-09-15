use std::fmt;

/// iceconnection_state indicates signaling state of the ICE Connection.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ICEConnectionState {
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

impl Default for ICEConnectionState {
    fn default() -> Self {
        ICEConnectionState::Unspecified
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
impl From<&str> for ICEConnectionState {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_CONNECTION_STATE_NEW_STR => ICEConnectionState::New,
            ICE_CONNECTION_STATE_CHECKING_STR => ICEConnectionState::Checking,
            ICE_CONNECTION_STATE_CONNECTED_STR => ICEConnectionState::Connected,
            ICE_CONNECTION_STATE_COMPLETED_STR => ICEConnectionState::Completed,
            ICE_CONNECTION_STATE_DISCONNECTED_STR => ICEConnectionState::Disconnected,
            ICE_CONNECTION_STATE_FAILED_STR => ICEConnectionState::Failed,
            ICE_CONNECTION_STATE_CLOSED_STR => ICEConnectionState::Closed,
            _ => ICEConnectionState::Unspecified,
        }
    }
}

impl From<u8> for ICEConnectionState {
    fn from(v: u8) -> Self {
        match v {
            1 => ICEConnectionState::New,
            2 => ICEConnectionState::Checking,
            3 => ICEConnectionState::Connected,
            4 => ICEConnectionState::Completed,
            5 => ICEConnectionState::Disconnected,
            6 => ICEConnectionState::Failed,
            7 => ICEConnectionState::Closed,
            _ => ICEConnectionState::Unspecified,
        }
    }
}

impl fmt::Display for ICEConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ICEConnectionState::New => ICE_CONNECTION_STATE_NEW_STR,
            ICEConnectionState::Checking => ICE_CONNECTION_STATE_CHECKING_STR,
            ICEConnectionState::Connected => ICE_CONNECTION_STATE_CONNECTED_STR,
            ICEConnectionState::Completed => ICE_CONNECTION_STATE_COMPLETED_STR,
            ICEConnectionState::Disconnected => ICE_CONNECTION_STATE_DISCONNECTED_STR,
            ICEConnectionState::Failed => ICE_CONNECTION_STATE_FAILED_STR,
            ICEConnectionState::Closed => ICE_CONNECTION_STATE_CLOSED_STR,
            ICEConnectionState::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_ice_connection_state() {
        let tests = vec![
            (crate::UNSPECIFIED_STR, ICEConnectionState::Unspecified),
            ("new", ICEConnectionState::New),
            ("checking", ICEConnectionState::Checking),
            ("connected", ICEConnectionState::Connected),
            ("completed", ICEConnectionState::Completed),
            ("disconnected", ICEConnectionState::Disconnected),
            ("failed", ICEConnectionState::Failed),
            ("closed", ICEConnectionState::Closed),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(
                expected_state,
                ICEConnectionState::from(state_string),
                "testCase: {}",
                expected_state,
            );
        }
    }

    #[test]
    fn test_ice_connection_state_string() {
        let tests = vec![
            (ICEConnectionState::Unspecified, crate::UNSPECIFIED_STR),
            (ICEConnectionState::New, "new"),
            (ICEConnectionState::Checking, "checking"),
            (ICEConnectionState::Connected, "connected"),
            (ICEConnectionState::Completed, "completed"),
            (ICEConnectionState::Disconnected, "disconnected"),
            (ICEConnectionState::Failed, "failed"),
            (ICEConnectionState::Closed, "closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(expected_string, state.to_string(),)
        }
    }
}
