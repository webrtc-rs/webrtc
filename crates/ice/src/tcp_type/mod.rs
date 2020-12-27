#[cfg(test)]
mod tcp_type_test;

use std::fmt;

// TCPType is the type of ICE TCP candidate as described in
// ttps://tools.ietf.org/html/rfc6544#section-4.5
#[derive(PartialEq, Debug)]
pub enum TCPType {
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
impl From<&str> for TCPType {
    fn from(raw: &str) -> Self {
        match raw {
            "active" => TCPType::Active,
            "passive" => TCPType::Passive,
            "so" => TCPType::SimultaneousOpen,
            _ => TCPType::Unspecified,
        }
    }
}

impl fmt::Display for TCPType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            TCPType::Active => "active",
            TCPType::Passive => "passive",
            TCPType::SimultaneousOpen => "so",
            TCPType::Unspecified => "unspecified",
        };
        write!(f, "{}", s)
    }
}
