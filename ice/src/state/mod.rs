#[cfg(test)]
mod state_test;

use std::fmt;

/// An enum showing the state of a ICE Connection List of supported States.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Unspecified,

    /// ICE agent is gathering addresses.
    New,

    /// ICE agent has been given local and remote candidates, and is attempting to find a match.
    Checking,

    /// ICE agent has a pairing, but is still checking other pairs.
    Connected,

    /// ICE agent has finished.
    Completed,

    /// ICE agent never could successfully connect.
    Failed,

    /// ICE agent connected successfully, but has entered a failed state.
    Disconnected,

    /// ICE agent has finished and is no longer handling requests.
    Closed,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Unspecified
    }
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Self::Unspecified => "Unspecified",
            Self::New => "New",
            Self::Checking => "Checking",
            Self::Connected => "Connected",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Disconnected => "Disconnected",
            Self::Closed => "Closed",
        };
        write!(f, "{s}")
    }
}

impl From<u8> for ConnectionState {
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

/// Describes the state of the candidate gathering process.
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum GatheringState {
    Unspecified,

    /// Indicates candidate gathering is not yet started.
    New,

    /// Indicates candidate gathering is ongoing.
    Gathering,

    /// Indicates candidate gathering has been completed.
    Complete,
}

impl From<u8> for GatheringState {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::New,
            2 => Self::Gathering,
            3 => Self::Complete,
            _ => Self::Unspecified,
        }
    }
}

impl Default for GatheringState {
    fn default() -> Self {
        Self::Unspecified
    }
}

impl fmt::Display for GatheringState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Self::New => "new",
            Self::Gathering => "gathering",
            Self::Complete => "complete",
            Self::Unspecified => "unspecified",
        };
        write!(f, "{s}")
    }
}
