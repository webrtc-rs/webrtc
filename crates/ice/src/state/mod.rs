use std::fmt;

// ConnectionState is an enum showing the state of a ICE Connection
// List of supported States
pub enum ConnectionState {
    // ConnectionStateNew ICE agent is gathering addresses
    New,

    // ConnectionStateChecking ICE agent has been given local and remote candidates, and is attempting to find a match
    Checking,

    // ConnectionStateConnected ICE agent has a pairing, but is still checking other pairs
    Connected,

    // ConnectionStateCompleted ICE agent has finished
    Completed,

    // ConnectionStateFailed ICE agent never could successfully connect
    Failed,

    // ConnectionStateDisconnected ICE agent connected successfully, but has entered a failed state
    Disconnected,

    // ConnectionStateClosed ICE agent has finished and is no longer handling requests
    Closed,
}

impl Default for ConnectionState {
    fn default() -> Self {
        ConnectionState::New
    }
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ConnectionState::New => "New",
            ConnectionState::Checking => "Checking",
            ConnectionState::Connected => "Connected",
            ConnectionState::Completed => "Completed",
            ConnectionState::Failed => "Failed",
            ConnectionState::Disconnected => "Disconnected",
            ConnectionState::Closed => "Closed",
        };
        write!(f, "{}", s)
    }
}

// GatheringState describes the state of the candidate gathering process
pub enum GatheringState {
    // GatheringStateNew indicates candidate gatering is not yet started
    New,

    // GatheringStateGathering indicates candidate gatering is ongoing
    Gathering,

    // GatheringStateComplete indicates candidate gatering has been completed
    Complete,
}

impl Default for GatheringState {
    fn default() -> Self {
        GatheringState::New
    }
}

impl fmt::Display for GatheringState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            GatheringState::New => "new",
            GatheringState::Gathering => "gathering",
            GatheringState::Complete => "complete",
        };
        write!(f, "{}", s)
    }
}
