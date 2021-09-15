use std::fmt;

/// DataChannelState indicates the state of a data channel.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DataChannelState {
    Unspecified = 0,

    /// DataChannelStateConnecting indicates that the data channel is being
    /// established. This is the initial state of DataChannel, whether created
    /// with create_data_channel, or dispatched as a part of an DataChannelEvent.
    Connecting,

    /// DataChannelStateOpen indicates that the underlying data transport is
    /// established and communication is possible.
    Open,

    /// DataChannelStateClosing indicates that the procedure to close down the
    /// underlying data transport has started.
    Closing,

    /// DataChannelStateClosed indicates that the underlying data transport
    /// has been closed or could not be established.
    Closed,
}

impl Default for DataChannelState {
    fn default() -> Self {
        DataChannelState::Unspecified
    }
}

const DATA_CHANNEL_STATE_CONNECTING_STR: &str = "connecting";
const DATA_CHANNEL_STATE_OPEN_STR: &str = "open";
const DATA_CHANNEL_STATE_CLOSING_STR: &str = "closing";
const DATA_CHANNEL_STATE_CLOSED_STR: &str = "closed";

impl From<u8> for DataChannelState {
    fn from(v: u8) -> Self {
        match v {
            1 => DataChannelState::Connecting,
            2 => DataChannelState::Open,
            3 => DataChannelState::Closing,
            4 => DataChannelState::Closed,
            _ => DataChannelState::Unspecified,
        }
    }
}

impl From<&str> for DataChannelState {
    fn from(raw: &str) -> Self {
        match raw {
            DATA_CHANNEL_STATE_CONNECTING_STR => DataChannelState::Connecting,
            DATA_CHANNEL_STATE_OPEN_STR => DataChannelState::Open,
            DATA_CHANNEL_STATE_CLOSING_STR => DataChannelState::Closing,
            DATA_CHANNEL_STATE_CLOSED_STR => DataChannelState::Closed,
            _ => DataChannelState::Unspecified,
        }
    }
}

impl fmt::Display for DataChannelState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            DataChannelState::Connecting => DATA_CHANNEL_STATE_CONNECTING_STR,
            DataChannelState::Open => DATA_CHANNEL_STATE_OPEN_STR,
            DataChannelState::Closing => DATA_CHANNEL_STATE_CLOSING_STR,
            DataChannelState::Closed => DATA_CHANNEL_STATE_CLOSED_STR,
            DataChannelState::Unspecified => crate::UNSPECIFIED_STR,
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_data_channel_state() {
        let tests = vec![
            (crate::UNSPECIFIED_STR, DataChannelState::Unspecified),
            ("connecting", DataChannelState::Connecting),
            ("open", DataChannelState::Open),
            ("closing", DataChannelState::Closing),
            ("closed", DataChannelState::Closed),
        ];

        for (state_string, expected_state) in tests {
            assert_eq!(
                expected_state,
                DataChannelState::from(state_string),
                "testCase: {}",
                expected_state,
            );
        }
    }

    #[test]
    fn test_data_channel_state_string() {
        let tests = vec![
            (DataChannelState::Unspecified, crate::UNSPECIFIED_STR),
            (DataChannelState::Connecting, "connecting"),
            (DataChannelState::Open, "open"),
            (DataChannelState::Closing, "closing"),
            (DataChannelState::Closed, "closed"),
        ];

        for (state, expected_string) in tests {
            assert_eq!(expected_string, state.to_string(),)
        }
    }
}
