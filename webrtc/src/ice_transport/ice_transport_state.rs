use ice::state::ConnectionState;
use std::fmt;

/// ICETransportState represents the current state of the ICE transport.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCIceTransportState {
    Unspecified,

    /// ICETransportStateNew indicates the ICETransport is waiting
    /// for remote candidates to be supplied.
    New,

    /// ICETransportStateChecking indicates the ICETransport has
    /// received at least one remote candidate, and a local and remote
    /// ICECandidateComplete dictionary was not added as the last candidate.
    Checking,

    /// ICETransportStateConnected indicates the ICETransport has
    /// received a response to an outgoing connectivity check, or has
    /// received incoming DTLS/media after a successful response to an
    /// incoming connectivity check, but is still checking other candidate
    /// pairs to see if there is a better connection.
    Connected,

    /// ICETransportStateCompleted indicates the ICETransport tested
    /// all appropriate candidate pairs and at least one functioning
    /// candidate pair has been found.
    Completed,

    /// ICETransportStateFailed indicates the ICETransport the last
    /// candidate was added and all appropriate candidate pairs have either
    /// failed connectivity checks or have lost consent.
    Failed,

    /// ICETransportStateDisconnected indicates the ICETransport has received
    /// at least one local and remote candidate, but the final candidate was
    /// received yet and all appropriate candidate pairs thus far have been
    /// tested and failed.
    Disconnected,

    /// ICETransportStateClosed indicates the ICETransport has shut down
    /// and is no longer responding to STUN requests.
    Closed,
}

impl Default for RTCIceTransportState {
    fn default() -> Self {
        RTCIceTransportState::Unspecified
    }
}

const ICE_TRANSPORT_STATE_NEW_STR: &str = "new";
const ICE_TRANSPORT_STATE_CHECKING_STR: &str = "checking";
const ICE_TRANSPORT_STATE_CONNECTED_STR: &str = "connected";
const ICE_TRANSPORT_STATE_COMPLETED_STR: &str = "completed";
const ICE_TRANSPORT_STATE_FAILED_STR: &str = "failed";
const ICE_TRANSPORT_STATE_DISCONNECTED_STR: &str = "disconnected";
const ICE_TRANSPORT_STATE_CLOSED_STR: &str = "closed";

impl From<&str> for RTCIceTransportState {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_TRANSPORT_STATE_NEW_STR => RTCIceTransportState::New,
            ICE_TRANSPORT_STATE_CHECKING_STR => RTCIceTransportState::Checking,
            ICE_TRANSPORT_STATE_CONNECTED_STR => RTCIceTransportState::Connected,
            ICE_TRANSPORT_STATE_COMPLETED_STR => RTCIceTransportState::Completed,
            ICE_TRANSPORT_STATE_FAILED_STR => RTCIceTransportState::Failed,
            ICE_TRANSPORT_STATE_DISCONNECTED_STR => RTCIceTransportState::Disconnected,
            ICE_TRANSPORT_STATE_CLOSED_STR => RTCIceTransportState::Closed,
            _ => RTCIceTransportState::Unspecified,
        }
    }
}

impl From<u8> for RTCIceTransportState {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::New,
            2 => Self::Checking,
            3 => Self::Connected,
            4 => Self::Completed,
            5 => Self::Failed,
            6 => Self::Disconnected,
            7 => Self::Closed,
            _ => Self::Unspecified,
        }
    }
}

impl fmt::Display for RTCIceTransportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RTCIceTransportState::New => write!(f, "{ICE_TRANSPORT_STATE_NEW_STR}"),
            RTCIceTransportState::Checking => write!(f, "{ICE_TRANSPORT_STATE_CHECKING_STR}"),
            RTCIceTransportState::Connected => {
                write!(f, "{ICE_TRANSPORT_STATE_CONNECTED_STR}")
            }
            RTCIceTransportState::Completed => write!(f, "{ICE_TRANSPORT_STATE_COMPLETED_STR}"),
            RTCIceTransportState::Failed => {
                write!(f, "{ICE_TRANSPORT_STATE_FAILED_STR}")
            }
            RTCIceTransportState::Disconnected => {
                write!(f, "{ICE_TRANSPORT_STATE_DISCONNECTED_STR}")
            }
            RTCIceTransportState::Closed => {
                write!(f, "{ICE_TRANSPORT_STATE_CLOSED_STR}")
            }
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

impl From<ConnectionState> for RTCIceTransportState {
    fn from(raw: ConnectionState) -> Self {
        match raw {
            ConnectionState::New => RTCIceTransportState::New,
            ConnectionState::Checking => RTCIceTransportState::Checking,
            ConnectionState::Connected => RTCIceTransportState::Connected,
            ConnectionState::Completed => RTCIceTransportState::Completed,
            ConnectionState::Failed => RTCIceTransportState::Failed,
            ConnectionState::Disconnected => RTCIceTransportState::Disconnected,
            ConnectionState::Closed => RTCIceTransportState::Closed,
            _ => RTCIceTransportState::Unspecified,
        }
    }
}

impl RTCIceTransportState {
    pub(crate) fn to_ice(self) -> ConnectionState {
        match self {
            RTCIceTransportState::New => ConnectionState::New,
            RTCIceTransportState::Checking => ConnectionState::Checking,
            RTCIceTransportState::Connected => ConnectionState::Connected,
            RTCIceTransportState::Completed => ConnectionState::Completed,
            RTCIceTransportState::Failed => ConnectionState::Failed,
            RTCIceTransportState::Disconnected => ConnectionState::Disconnected,
            RTCIceTransportState::Closed => ConnectionState::Closed,
            _ => ConnectionState::Unspecified,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ice_transport_state_string() {
        let tests = vec![
            (RTCIceTransportState::Unspecified, "Unspecified"),
            (RTCIceTransportState::New, "new"),
            (RTCIceTransportState::Checking, "checking"),
            (RTCIceTransportState::Connected, "connected"),
            (RTCIceTransportState::Completed, "completed"),
            (RTCIceTransportState::Failed, "failed"),
            (RTCIceTransportState::Disconnected, "disconnected"),
            (RTCIceTransportState::Closed, "closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(state.to_string(), expected_string);
        }
    }

    #[test]
    fn test_ice_transport_state_convert() {
        let tests = vec![
            (
                RTCIceTransportState::Unspecified,
                ConnectionState::Unspecified,
            ),
            (RTCIceTransportState::New, ConnectionState::New),
            (RTCIceTransportState::Checking, ConnectionState::Checking),
            (RTCIceTransportState::Connected, ConnectionState::Connected),
            (RTCIceTransportState::Completed, ConnectionState::Completed),
            (RTCIceTransportState::Failed, ConnectionState::Failed),
            (
                RTCIceTransportState::Disconnected,
                ConnectionState::Disconnected,
            ),
            (RTCIceTransportState::Closed, ConnectionState::Closed),
        ];

        for (native, ice_state) in tests {
            assert_eq!(native.to_ice(), ice_state);
            assert_eq!(native, RTCIceTransportState::from(ice_state));
        }
    }
}
