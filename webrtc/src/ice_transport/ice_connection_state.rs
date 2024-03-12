use std::fmt;

/// Indicates the signaling state of the ICE Connection.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCIceConnectionState {
    #[default]
    Unspecified,

    /// Indicates that the [`RTCPeerConnection`] is closed
    ///
    /// [`RTCPeerConnection`]: crate::peer_connection::RTCPeerConnection
    Closed,

    /// Indicates all of the following:
    /// - the previous state does not apply
    /// - any of the [`RTCIceTransport`]s are in the `failed` state
    ///
    /// [`RTCIceTransport`]: crate::ice_transport::RTCIceTransport
    Failed,

    /// Indicates all of the following:
    /// - none of the previous states apply
    /// - any of the [`RTCIceTransport`]s are in the `disconnected` state
    ///
    /// [`RTCIceTransport`]: crate::ice_transport::RTCIceTransport
    Disconnected,

    /// Indicates all of the following:
    /// - none of the previous states apply
    /// - all of the [`RTCIceTransport`]s are in the `new` or
    /// `closed` state, or
    /// - there are no transports
    ///
    /// [`RTCIceTransport`]: crate::ice_transport::RTCIceTransport
    New,

    /// Indicates all of the following:
    /// - none of the previous states apply
    /// - any of the [`RTCIceTransport`]s are in the `new` or `checking` state
    ///
    /// [`RTCIceTransport`]: crate::ice_transport::RTCIceTransport
    Checking,

    /// Indicates all of the following:
    /// - none of the previous states apply
    /// - all of the [`RTCIceTransport`]s are in the `completed` or
    /// `closed` state
    ///
    /// [`RTCIceTransport`]: crate::ice_transport::RTCIceTransport
    Completed,

    /// Indicates all of the following:
    /// - none of the previous states apply
    /// - all of the [`RTCIceTransport`]s are in the `connected`, `completed`, or
    /// `closed` state
    ///
    /// [`RTCIceTransport`]: crate::ice_transport::RTCIceTransport
    Connected,
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
            1 => RTCIceConnectionState::Closed,
            2 => RTCIceConnectionState::Failed,
            3 => RTCIceConnectionState::Disconnected,
            4 => RTCIceConnectionState::New,
            5 => RTCIceConnectionState::Checking,
            6 => RTCIceConnectionState::Completed,
            7 => RTCIceConnectionState::Connected,
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
