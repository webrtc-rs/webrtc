use ice::state::ConnectionState;
use std::fmt;

/// ICETransportState represents the current state of the ICE transport.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ICETransportState {
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

impl Default for ICETransportState {
    fn default() -> Self {
        ICETransportState::Unspecified
    }
}

const ICE_TRANSPORT_STATE_NEW_STR: &str = "New";
const ICE_TRANSPORT_STATE_CHECKING_STR: &str = "Checking";
const ICE_TRANSPORT_STATE_CONNECTED_STR: &str = "Connected";
const ICE_TRANSPORT_STATE_COMPLETED_STR: &str = "Completed";
const ICE_TRANSPORT_STATE_FAILED_STR: &str = "Failed";
const ICE_TRANSPORT_STATE_DISCONNECTED_STR: &str = "Disconnected";
const ICE_TRANSPORT_STATE_CLOSED_STR: &str = "Closed";

impl From<&str> for ICETransportState {
    fn from(raw: &str) -> Self {
        match raw {
            ICE_TRANSPORT_STATE_NEW_STR => ICETransportState::New,
            ICE_TRANSPORT_STATE_CHECKING_STR => ICETransportState::Checking,
            ICE_TRANSPORT_STATE_CONNECTED_STR => ICETransportState::Connected,
            ICE_TRANSPORT_STATE_COMPLETED_STR => ICETransportState::Completed,
            ICE_TRANSPORT_STATE_FAILED_STR => ICETransportState::Failed,
            ICE_TRANSPORT_STATE_DISCONNECTED_STR => ICETransportState::Disconnected,
            ICE_TRANSPORT_STATE_CLOSED_STR => ICETransportState::Closed,
            _ => ICETransportState::Unspecified,
        }
    }
}

impl From<u8> for ICETransportState {
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

impl fmt::Display for ICETransportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ICETransportState::New => write!(f, "{}", ICE_TRANSPORT_STATE_NEW_STR),
            ICETransportState::Checking => write!(f, "{}", ICE_TRANSPORT_STATE_CHECKING_STR),
            ICETransportState::Connected => {
                write!(f, "{}", ICE_TRANSPORT_STATE_CONNECTED_STR)
            }
            ICETransportState::Completed => write!(f, "{}", ICE_TRANSPORT_STATE_COMPLETED_STR),
            ICETransportState::Failed => {
                write!(f, "{}", ICE_TRANSPORT_STATE_FAILED_STR)
            }
            ICETransportState::Disconnected => {
                write!(f, "{}", ICE_TRANSPORT_STATE_DISCONNECTED_STR)
            }
            ICETransportState::Closed => {
                write!(f, "{}", ICE_TRANSPORT_STATE_CLOSED_STR)
            }
            _ => write!(f, "{}", crate::UNSPECIFIED_STR),
        }
    }
}

impl From<ConnectionState> for ICETransportState {
    fn from(raw: ConnectionState) -> Self {
        match raw {
            ConnectionState::New => ICETransportState::New,
            ConnectionState::Checking => ICETransportState::Checking,
            ConnectionState::Connected => ICETransportState::Connected,
            ConnectionState::Completed => ICETransportState::Completed,
            ConnectionState::Failed => ICETransportState::Failed,
            ConnectionState::Disconnected => ICETransportState::Disconnected,
            ConnectionState::Closed => ICETransportState::Closed,
            _ => ICETransportState::Unspecified,
        }
    }
}

impl ICETransportState {
    pub(crate) fn to_ice(self) -> ConnectionState {
        match self {
            ICETransportState::New => ConnectionState::New,
            ICETransportState::Checking => ConnectionState::Checking,
            ICETransportState::Connected => ConnectionState::Connected,
            ICETransportState::Completed => ConnectionState::Completed,
            ICETransportState::Failed => ConnectionState::Failed,
            ICETransportState::Disconnected => ConnectionState::Disconnected,
            ICETransportState::Closed => ConnectionState::Closed,
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
            (ICETransportState::Unspecified, "Unspecified"),
            (ICETransportState::New, "New"),
            (ICETransportState::Checking, "Checking"),
            (ICETransportState::Connected, "Connected"),
            (ICETransportState::Completed, "Completed"),
            (ICETransportState::Failed, "Failed"),
            (ICETransportState::Disconnected, "Disconnected"),
            (ICETransportState::Closed, "Closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(expected_string, state.to_string());
        }
    }

    #[test]
    fn test_ice_transport_state_convert() {
        let tests = vec![
            (ICETransportState::Unspecified, ConnectionState::Unspecified),
            (ICETransportState::New, ConnectionState::New),
            (ICETransportState::Checking, ConnectionState::Checking),
            (ICETransportState::Connected, ConnectionState::Connected),
            (ICETransportState::Completed, ConnectionState::Completed),
            (ICETransportState::Failed, ConnectionState::Failed),
            (
                ICETransportState::Disconnected,
                ConnectionState::Disconnected,
            ),
            (ICETransportState::Closed, ConnectionState::Closed),
        ];

        for (native, ice_state) in tests {
            assert_eq!(native.to_ice(), ice_state);
            assert_eq!(native, ICETransportState::from(ice_state));
        }
    }
}
