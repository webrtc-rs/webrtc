use std::fmt;

/// DTLSTransportState indicates the DTLS transport establishment state.
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCDtlsTransport/state
/// [W3C]: https://w3c.github.io/webrtc-pc/#dom-rtcdtlstransportstate
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCDtlsTransportState {
    #[default]
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

const DTLS_TRANSPORT_STATE_NEW_STR: &str = "new";
const DTLS_TRANSPORT_STATE_CONNECTING_STR: &str = "connecting";
const DTLS_TRANSPORT_STATE_CONNECTED_STR: &str = "connected";
const DTLS_TRANSPORT_STATE_CLOSED_STR: &str = "closed";
const DTLS_TRANSPORT_STATE_FAILED_STR: &str = "failed";

impl From<&str> for RTCDtlsTransportState {
    fn from(raw: &str) -> Self {
        match raw {
            DTLS_TRANSPORT_STATE_NEW_STR => RTCDtlsTransportState::New,
            DTLS_TRANSPORT_STATE_CONNECTING_STR => RTCDtlsTransportState::Connecting,
            DTLS_TRANSPORT_STATE_CONNECTED_STR => RTCDtlsTransportState::Connected,
            DTLS_TRANSPORT_STATE_CLOSED_STR => RTCDtlsTransportState::Closed,
            DTLS_TRANSPORT_STATE_FAILED_STR => RTCDtlsTransportState::Failed,
            _ => RTCDtlsTransportState::Unspecified,
        }
    }
}

impl From<u8> for RTCDtlsTransportState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCDtlsTransportState::New,
            2 => RTCDtlsTransportState::Connecting,
            3 => RTCDtlsTransportState::Connected,
            4 => RTCDtlsTransportState::Closed,
            5 => RTCDtlsTransportState::Failed,
            _ => RTCDtlsTransportState::Unspecified,
        }
    }
}

impl fmt::Display for RTCDtlsTransportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCDtlsTransportState::New => DTLS_TRANSPORT_STATE_NEW_STR,
            RTCDtlsTransportState::Connecting => DTLS_TRANSPORT_STATE_CONNECTING_STR,
            RTCDtlsTransportState::Connected => DTLS_TRANSPORT_STATE_CONNECTED_STR,
            RTCDtlsTransportState::Closed => DTLS_TRANSPORT_STATE_CLOSED_STR,
            RTCDtlsTransportState::Failed => DTLS_TRANSPORT_STATE_FAILED_STR,
            RTCDtlsTransportState::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_dtls_transport_state() {
        let tests = vec![
            (crate::UNSPECIFIED_STR, RTCDtlsTransportState::Unspecified),
            ("new", RTCDtlsTransportState::New),
            ("connecting", RTCDtlsTransportState::Connecting),
            ("connected", RTCDtlsTransportState::Connected),
            ("closed", RTCDtlsTransportState::Closed),
            ("failed", RTCDtlsTransportState::Failed),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(
                RTCDtlsTransportState::from(state_string),
                expected_state,
                "testCase: {expected_state}",
            );
        }
    }

    #[test]
    fn test_dtls_transport_state_string() {
        let tests = vec![
            (RTCDtlsTransportState::Unspecified, crate::UNSPECIFIED_STR),
            (RTCDtlsTransportState::New, "new"),
            (RTCDtlsTransportState::Connecting, "connecting"),
            (RTCDtlsTransportState::Connected, "connected"),
            (RTCDtlsTransportState::Closed, "closed"),
            (RTCDtlsTransportState::Failed, "failed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(state.to_string(), expected_string)
        }
    }
}
