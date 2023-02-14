use std::fmt;

#[cfg(test)]
mod direction_test;

/// Direction is a marker for transmission direction of an endpoint
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Direction {
    Unspecified = 0,
    /// Direction::SendRecv is for bidirectional communication
    SendRecv = 1,
    /// Direction::SendOnly is for outgoing communication
    SendOnly = 2,
    /// Direction::RecvOnly is for incoming communication
    RecvOnly = 3,
    /// Direction::Inactive is for no communication
    Inactive = 4,
}

const DIRECTION_SEND_RECV_STR: &str = "sendrecv";
const DIRECTION_SEND_ONLY_STR: &str = "sendonly";
const DIRECTION_RECV_ONLY_STR: &str = "recvonly";
const DIRECTION_INACTIVE_STR: &str = "inactive";
const DIRECTION_UNSPECIFIED_STR: &str = "Unspecified";

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Direction::SendRecv => DIRECTION_SEND_RECV_STR,
            Direction::SendOnly => DIRECTION_SEND_ONLY_STR,
            Direction::RecvOnly => DIRECTION_RECV_ONLY_STR,
            Direction::Inactive => DIRECTION_INACTIVE_STR,
            _ => DIRECTION_UNSPECIFIED_STR,
        };
        write!(f, "{s}")
    }
}

impl Default for Direction {
    fn default() -> Direction {
        Direction::Unspecified
    }
}

impl Direction {
    /// new defines a procedure for creating a new direction from a raw string.
    pub fn new(raw: &str) -> Self {
        match raw {
            DIRECTION_SEND_RECV_STR => Direction::SendRecv,
            DIRECTION_SEND_ONLY_STR => Direction::SendOnly,
            DIRECTION_RECV_ONLY_STR => Direction::RecvOnly,
            DIRECTION_INACTIVE_STR => Direction::Inactive,
            _ => Direction::Unspecified,
        }
    }
}
