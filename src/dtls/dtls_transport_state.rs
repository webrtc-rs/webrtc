use std::fmt;

/// DTLSTransportState indicates the DTLS transport establishment state.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DTLSTransportState {
    Unspecified = 0,

    /// DTLSTransportStateNew indicates that DTLS has not started negotiating
    /// yet.
    New = 1,

    /// DTLSTransportStateConnecting indicates that DTLS is in the process of
    /// negotiating a secure connection and verifying the remote fingerprint.
    Connecting = 2,

    /// DTLSTransportStateConnected indicates that DTLS has completed
    /// negotiation of a secure connection and verified the remote fingerprint.
    Connected = 3,

    /// DTLSTransportStateClosed indicates that the transport has been closed
    /// intentionally as the result of receipt of a close_notify alert, or
    /// calling close().
    Closed = 4,

    /// DTLSTransportStateFailed indicates that the transport has failed as
    /// the result of an error (such as receipt of an error alert or failure to
    /// validate the remote fingerprint).
    Failed = 5,
}

impl Default for DTLSTransportState {
    fn default() -> Self {
        DTLSTransportState::Unspecified
    }
}

const DTLS_TRANSPORT_STATE_NEW_STR: &str = "New";
const DTLS_TRANSPORT_STATE_CONNECTING_STR: &str = "Connecting";
const DTLS_TRANSPORT_STATE_CONNECTED_STR: &str = "Connected";
const DTLS_TRANSPORT_STATE_CLOSED_STR: &str = "Closed";
const DTLS_TRANSPORT_STATE_FAILED_STR: &str = "Failed";

impl From<&str> for DTLSTransportState {
    fn from(raw: &str) -> Self {
        match raw {
            DTLS_TRANSPORT_STATE_NEW_STR => DTLSTransportState::New,
            DTLS_TRANSPORT_STATE_CONNECTING_STR => DTLSTransportState::Connecting,
            DTLS_TRANSPORT_STATE_CONNECTED_STR => DTLSTransportState::Connected,
            DTLS_TRANSPORT_STATE_CLOSED_STR => DTLSTransportState::Closed,
            DTLS_TRANSPORT_STATE_FAILED_STR => DTLSTransportState::Failed,
            _ => DTLSTransportState::Unspecified,
        }
    }
}

impl fmt::Display for DTLSTransportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            DTLSTransportState::New => DTLS_TRANSPORT_STATE_NEW_STR,
            DTLSTransportState::Connecting => DTLS_TRANSPORT_STATE_CONNECTING_STR,
            DTLSTransportState::Connected => DTLS_TRANSPORT_STATE_CONNECTED_STR,
            DTLSTransportState::Closed => DTLS_TRANSPORT_STATE_CLOSED_STR,
            DTLSTransportState::Failed => DTLS_TRANSPORT_STATE_FAILED_STR,
            DTLSTransportState::Unspecified => "Unspecified",
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_dtls_transport_state() {
        let tests = vec![
            ("Unspecified", DTLSTransportState::Unspecified),
            ("New", DTLSTransportState::New),
            ("Connecting", DTLSTransportState::Connecting),
            ("Connected", DTLSTransportState::Connected),
            ("Closed", DTLSTransportState::Closed),
            ("Failed", DTLSTransportState::Failed),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(
                expected_state,
                DTLSTransportState::from(state_string),
                "testCase: {}",
                expected_state,
            );
        }
    }

    #[test]
    fn test_dtls_transport_state_string() {
        let tests = vec![
            (DTLSTransportState::Unspecified, "Unspecified"),
            (DTLSTransportState::New, "New"),
            (DTLSTransportState::Connecting, "Connecting"),
            (DTLSTransportState::Connected, "Connected"),
            (DTLSTransportState::Closed, "Closed"),
            (DTLSTransportState::Failed, "Failed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(expected_string, state.to_string(),)
        }
    }
}
