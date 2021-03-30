#[cfg(test)]
mod tcp_type_test;

use std::fmt;

// TCPType is the type of ICE TCP candidate as described in
// ttps://tools.ietf.org/html/rfc6544#section-4.5
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum TcpType {
    // TCPTypeUnspecified is the default value. For example UDP candidates do not
    // need this field.
    Unspecified,
    // TCPTypeActive is active TCP candidate, which initiates TCP connections.
    Active,
    // TCPTypePassive is passive TCP candidate, only accepts TCP connections.
    Passive,
    // TCPTypeSimultaneousOpen is like active and passive at the same time.
    SimultaneousOpen,
}

// from creates a new TCPType from string.
impl From<&str> for TcpType {
    fn from(raw: &str) -> Self {
        match raw {
            "active" => TcpType::Active,
            "passive" => TcpType::Passive,
            "so" => TcpType::SimultaneousOpen,
            _ => TcpType::Unspecified,
        }
    }
}

impl fmt::Display for TcpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            TcpType::Active => "active",
            TcpType::Passive => "passive",
            TcpType::SimultaneousOpen => "so",
            TcpType::Unspecified => "unspecified",
        };
        write!(f, "{}", s)
    }
}

impl Default for TcpType {
    fn default() -> Self {
        TcpType::Unspecified
    }
}
