#[cfg(test)]
mod tcp_type_test;

use std::fmt;

/// TCPType is the type of ICE TCP candidate
///
/// ## Specifications
///
/// * [RFC 6544 §4.5]
///
/// [RFC 6544 §4.5]: https://tools.ietf.org/html/rfc6544#section-4.5
#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum TcpType {
    /// The default value. For example UDP candidates do not need this field.
    Unspecified,
    /// Active TCP candidate, which initiates TCP connections.
    Active,
    /// Passive TCP candidate, only accepts TCP connections.
    Passive,
    /// Like `Active` and `Passive` at the same time.
    SimultaneousOpen,
}

// from creates a new TCPType from string.
impl From<&str> for TcpType {
    fn from(raw: &str) -> Self {
        match raw {
            "active" => Self::Active,
            "passive" => Self::Passive,
            "so" => Self::SimultaneousOpen,
            _ => Self::Unspecified,
        }
    }
}

impl fmt::Display for TcpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Self::Active => "active",
            Self::Passive => "passive",
            Self::SimultaneousOpen => "so",
            Self::Unspecified => "unspecified",
        };
        write!(f, "{s}")
    }
}

impl Default for TcpType {
    fn default() -> Self {
        Self::Unspecified
    }
}
