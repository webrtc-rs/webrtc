use std::fmt;

/// SCTPTransportState indicates the state of the SCTP transport.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum RTCSctpTransportState {
    Unspecified,

    /// SCTPTransportStateConnecting indicates the SCTPTransport is in the
    /// process of negotiating an association. This is the initial state of the
    /// SCTPTransportState when an SCTPTransport is created.
    Connecting,

    /// SCTPTransportStateConnected indicates the negotiation of an
    /// association is completed.
    Connected,

    /// SCTPTransportStateClosed indicates a SHUTDOWN or ABORT chunk is
    /// received or when the SCTP association has been closed intentionally,
    /// such as by closing the peer connection or applying a remote description
    /// that rejects data or changes the SCTP port.
    Closed,
}

impl Default for RTCSctpTransportState {
    fn default() -> Self {
        RTCSctpTransportState::Unspecified
    }
}

const SCTP_TRANSPORT_STATE_CONNECTING_STR: &str = "connecting";
const SCTP_TRANSPORT_STATE_CONNECTED_STR: &str = "connected";
const SCTP_TRANSPORT_STATE_CLOSED_STR: &str = "closed";

impl From<&str> for RTCSctpTransportState {
    fn from(raw: &str) -> Self {
        match raw {
            SCTP_TRANSPORT_STATE_CONNECTING_STR => RTCSctpTransportState::Connecting,
            SCTP_TRANSPORT_STATE_CONNECTED_STR => RTCSctpTransportState::Connected,
            SCTP_TRANSPORT_STATE_CLOSED_STR => RTCSctpTransportState::Closed,
            _ => RTCSctpTransportState::Unspecified,
        }
    }
}

impl From<u8> for RTCSctpTransportState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCSctpTransportState::Connecting,
            2 => RTCSctpTransportState::Connected,
            3 => RTCSctpTransportState::Closed,
            _ => RTCSctpTransportState::Unspecified,
        }
    }
}

impl fmt::Display for RTCSctpTransportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCSctpTransportState::Connecting => SCTP_TRANSPORT_STATE_CONNECTING_STR,
            RTCSctpTransportState::Connected => SCTP_TRANSPORT_STATE_CONNECTED_STR,
            RTCSctpTransportState::Closed => SCTP_TRANSPORT_STATE_CLOSED_STR,
            RTCSctpTransportState::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_sctp_transport_state() {
        let tests = vec![
            (crate::UNSPECIFIED_STR, RTCSctpTransportState::Unspecified),
            ("connecting", RTCSctpTransportState::Connecting),
            ("connected", RTCSctpTransportState::Connected),
            ("closed", RTCSctpTransportState::Closed),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(
                RTCSctpTransportState::from(state_string),
                expected_state,
                "testCase: {expected_state}",
            );
        }
    }

    #[test]
    fn test_sctp_transport_state_string() {
        let tests = vec![
            (RTCSctpTransportState::Unspecified, crate::UNSPECIFIED_STR),
            (RTCSctpTransportState::Connecting, "connecting"),
            (RTCSctpTransportState::Connected, "connected"),
            (RTCSctpTransportState::Closed, "closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(state.to_string(), expected_string)
        }
    }
}
