pub mod addr;
pub mod chandata;
pub mod channum;
pub mod data;
pub mod dontfrag;

use std::fmt;

// Protocol is IANA assigned protocol number.
#[derive(PartialEq, Eq, Default)]
pub struct Protocol(pub u8);

// PROTO_UDP is IANA assigned protocol number for UDP.
pub const PROTO_UDP: Protocol = Protocol(17);

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let others = format!("{}", self.0);
        let s = match *self {
            PROTO_UDP => "UDP",
            _ => others.as_str(),
        };

        write!(f, "{}", s)
    }
}
