use std::fmt;

#[cfg(test)]
mod direction_test;

//Direction is a marker for transmission direction of an endpoint
#[derive(Debug, PartialEq, Clone)]
pub enum Direction {
    DirectionUnknown = 0,
    //DirectionSendRecv is for bidirectional communication
    DirectionSendRecv = 1,
    //DirectionSendOnly is for outgoing communication
    DirectionSendOnly = 2,
    //DirectionRecvOnly is for incoming communication
    DirectionRecvOnly = 3,
    //DirectionInactive is for no communication
    DirectionInactive = 4,
}

pub const DIRECTION_SEND_RECV_STR: &'static str = "sendrecv";
pub const DIRECTION_SEND_ONLY_STR: &'static str = "sendonly";
pub const DIRECTION_RECV_ONLY_STR: &'static str = "recvonly";
pub const DIRECTION_INACTIVE_STR: &'static str = "inactive";
pub const DIRECTION_UNKNOWN_STR: &'static str = "";

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Direction::DirectionSendRecv => DIRECTION_SEND_RECV_STR,
            Direction::DirectionSendOnly => DIRECTION_SEND_ONLY_STR,
            Direction::DirectionRecvOnly => DIRECTION_RECV_ONLY_STR,
            Direction::DirectionInactive => DIRECTION_INACTIVE_STR,
            _ => DIRECTION_UNKNOWN_STR,
        };
        write!(f, "{}", s)
    }
}

impl Default for Direction {
    fn default() -> Direction {
        Direction::DirectionUnknown
    }
}
// NewDirection defines a procedure for creating a new direction from a raw string.
impl Direction {
    pub fn new(raw: &str) -> Self {
        match raw {
            DIRECTION_SEND_RECV_STR => Direction::DirectionSendRecv,
            DIRECTION_SEND_ONLY_STR => Direction::DirectionSendOnly,
            DIRECTION_RECV_ONLY_STR => Direction::DirectionRecvOnly,
            DIRECTION_INACTIVE_STR => Direction::DirectionInactive,
            _ => Direction::DirectionUnknown,
        }
    }
}
