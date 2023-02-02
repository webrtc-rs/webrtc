#[cfg(test)]
mod proto_test;

pub mod addr;
pub mod chandata;
pub mod channum;
pub mod data;
pub mod dontfrag;
pub mod evenport;
pub mod lifetime;
pub mod peeraddr;
pub mod relayaddr;
pub mod reqfamily;
pub mod reqtrans;
pub mod rsrvtoken;

use std::fmt;

use stun::message::*;

// proto implements RFC 5766 Traversal Using Relays around NAT.

// protocol is IANA assigned protocol number.
#[derive(PartialEq, Eq, Default, Debug, Clone, Copy, Hash)]
pub struct Protocol(pub u8);

// PROTO_UDP is IANA assigned protocol number for UDP.
pub const PROTO_TCP: Protocol = Protocol(6);
pub const PROTO_UDP: Protocol = Protocol(17);

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let others = format!("{}", self.0);
        let s = match *self {
            PROTO_UDP => "UDP",
            PROTO_TCP => "TCP",
            _ => others.as_str(),
        };

        write!(f, "{s}")
    }
}

// Default ports for TURN from RFC 5766 Section 4.

// DEFAULT_PORT for TURN is same as STUN.
pub const DEFAULT_PORT: u16 = stun::DEFAULT_PORT;
// DEFAULT_TLSPORT is for TURN over TLS and is same as STUN.
pub const DEFAULT_TLS_PORT: u16 = stun::DEFAULT_TLS_PORT;

// create_permission_request is shorthand for create permission request type.
pub fn create_permission_request() -> MessageType {
    MessageType::new(METHOD_CREATE_PERMISSION, CLASS_REQUEST)
}

// allocate_request is shorthand for allocation request message type.
pub fn allocate_request() -> MessageType {
    MessageType::new(METHOD_ALLOCATE, CLASS_REQUEST)
}

// send_indication is shorthand for send indication message type.
pub fn send_indication() -> MessageType {
    MessageType::new(METHOD_SEND, CLASS_INDICATION)
}

// refresh_request is shorthand for refresh request message type.
pub fn refresh_request() -> MessageType {
    MessageType::new(METHOD_REFRESH, CLASS_REQUEST)
}
