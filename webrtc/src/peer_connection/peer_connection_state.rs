use std::fmt;

/// PeerConnectionState indicates the state of the PeerConnection.
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCPeerConnection/connectionState
/// [W3C]: https://w3c.github.io/webrtc-pc/#dom-peerconnection-connection-state
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum RTCPeerConnectionState {
    #[default]
    Unspecified,

    /// PeerConnectionStateNew indicates that any of the ICETransports or
    /// DTLSTransports are in the "new" state and none of the transports are
    /// in the "connecting", "checking", "failed" or "disconnected" state, or
    /// all transports are in the "closed" state, or there are no transports.
    New,

    /// PeerConnectionStateConnecting indicates that any of the
    /// ICETransports or DTLSTransports are in the "connecting" or
    /// "checking" state and none of them is in the "failed" state.
    Connecting,

    /// PeerConnectionStateConnected indicates that all ICETransports and
    /// DTLSTransports are in the "connected", "completed" or "closed" state
    /// and at least one of them is in the "connected" or "completed" state.
    Connected,

    /// PeerConnectionStateDisconnected indicates that any of the
    /// ICETransports or DTLSTransports are in the "disconnected" state
    /// and none of them are in the "failed" or "connecting" or "checking" state.
    Disconnected,

    /// PeerConnectionStateFailed indicates that any of the ICETransports
    /// or DTLSTransports are in a "failed" state.
    Failed,

    /// PeerConnectionStateClosed indicates the peer connection is closed
    /// and the isClosed member variable of PeerConnection is true.
    Closed,
}

const PEER_CONNECTION_STATE_NEW_STR: &str = "new";
const PEER_CONNECTION_STATE_CONNECTING_STR: &str = "connecting";
const PEER_CONNECTION_STATE_CONNECTED_STR: &str = "connected";
const PEER_CONNECTION_STATE_DISCONNECTED_STR: &str = "disconnected";
const PEER_CONNECTION_STATE_FAILED_STR: &str = "failed";
const PEER_CONNECTION_STATE_CLOSED_STR: &str = "closed";

impl From<&str> for RTCPeerConnectionState {
    fn from(raw: &str) -> Self {
        match raw {
            PEER_CONNECTION_STATE_NEW_STR => RTCPeerConnectionState::New,
            PEER_CONNECTION_STATE_CONNECTING_STR => RTCPeerConnectionState::Connecting,
            PEER_CONNECTION_STATE_CONNECTED_STR => RTCPeerConnectionState::Connected,
            PEER_CONNECTION_STATE_DISCONNECTED_STR => RTCPeerConnectionState::Disconnected,
            PEER_CONNECTION_STATE_FAILED_STR => RTCPeerConnectionState::Failed,
            PEER_CONNECTION_STATE_CLOSED_STR => RTCPeerConnectionState::Closed,
            _ => RTCPeerConnectionState::Unspecified,
        }
    }
}

impl From<u8> for RTCPeerConnectionState {
    fn from(v: u8) -> Self {
        match v {
            1 => RTCPeerConnectionState::New,
            2 => RTCPeerConnectionState::Connecting,
            3 => RTCPeerConnectionState::Connected,
            4 => RTCPeerConnectionState::Disconnected,
            5 => RTCPeerConnectionState::Failed,
            6 => RTCPeerConnectionState::Closed,
            _ => RTCPeerConnectionState::Unspecified,
        }
    }
}

impl fmt::Display for RTCPeerConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RTCPeerConnectionState::New => PEER_CONNECTION_STATE_NEW_STR,
            RTCPeerConnectionState::Connecting => PEER_CONNECTION_STATE_CONNECTING_STR,
            RTCPeerConnectionState::Connected => PEER_CONNECTION_STATE_CONNECTED_STR,
            RTCPeerConnectionState::Disconnected => PEER_CONNECTION_STATE_DISCONNECTED_STR,
            RTCPeerConnectionState::Failed => PEER_CONNECTION_STATE_FAILED_STR,
            RTCPeerConnectionState::Closed => PEER_CONNECTION_STATE_CLOSED_STR,
            RTCPeerConnectionState::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{s}")
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub(crate) enum NegotiationNeededState {
    /// NegotiationNeededStateEmpty not running and queue is empty
    #[default]
    Empty,
    /// NegotiationNeededStateEmpty running and queue is empty
    Run,
    /// NegotiationNeededStateEmpty running and queue
    Queue,
}

impl From<u8> for NegotiationNeededState {
    fn from(v: u8) -> Self {
        match v {
            1 => NegotiationNeededState::Run,
            2 => NegotiationNeededState::Queue,
            _ => NegotiationNeededState::Empty,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_peer_connection_state() {
        let tests = vec![
            (crate::UNSPECIFIED_STR, RTCPeerConnectionState::Unspecified),
            ("new", RTCPeerConnectionState::New),
            ("connecting", RTCPeerConnectionState::Connecting),
            ("connected", RTCPeerConnectionState::Connected),
            ("disconnected", RTCPeerConnectionState::Disconnected),
            ("failed", RTCPeerConnectionState::Failed),
            ("closed", RTCPeerConnectionState::Closed),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(
                RTCPeerConnectionState::from(state_string),
                expected_state,
                "testCase: {expected_state}",
            );
        }
    }

    #[test]
    fn test_peer_connection_state_string() {
        let tests = vec![
            (RTCPeerConnectionState::Unspecified, crate::UNSPECIFIED_STR),
            (RTCPeerConnectionState::New, "new"),
            (RTCPeerConnectionState::Connecting, "connecting"),
            (RTCPeerConnectionState::Connected, "connected"),
            (RTCPeerConnectionState::Disconnected, "disconnected"),
            (RTCPeerConnectionState::Failed, "failed"),
            (RTCPeerConnectionState::Closed, "closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(state.to_string(), expected_string)
        }
    }
}
