#[cfg(test)]
mod reqtrans_test;

use std::fmt;

use stun::attributes::*;
use stun::checks::*;
use stun::message::*;

use super::*;

/// `RequestedTransport` represents `REQUESTED-TRANSPORT` attribute.
///
/// This attribute is used by the client to request a specific transport
/// protocol for the allocated transport address. RFC 5766 only allows the use of
/// codepoint 17 (User Datagram protocol).
///
/// [RFC 5766 Section 14.7](https://www.rfc-editor.org/rfc/rfc5766#section-14.7).
#[derive(Default, Debug, PartialEq, Eq)]
pub struct RequestedTransport {
    pub protocol: Protocol,
}

impl fmt::Display for RequestedTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "protocol: {}", self.protocol)
    }
}

const REQUESTED_TRANSPORT_SIZE: usize = 4;

impl Setter for RequestedTransport {
    /// Adds `REQUESTED-TRANSPORT` to message.
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        let mut v = vec![0; REQUESTED_TRANSPORT_SIZE];
        v[0] = self.protocol.0;
        // b[1:4] is RFFU = 0.
        // The RFFU field MUST be set to zero on transmission and MUST be
        // ignored on reception. It is reserved for future uses.
        m.add(ATTR_REQUESTED_TRANSPORT, &v);
        Ok(())
    }
}

impl Getter for RequestedTransport {
    /// Decodes `REQUESTED-TRANSPORT` from message.
    fn get_from(&mut self, m: &Message) -> Result<(), stun::Error> {
        let v = m.get(ATTR_REQUESTED_TRANSPORT)?;

        check_size(ATTR_REQUESTED_TRANSPORT, v.len(), REQUESTED_TRANSPORT_SIZE)?;
        self.protocol = Protocol(v[0]);
        Ok(())
    }
}
