#[cfg(test)]
mod rsrvtoken_test;

use stun::attributes::*;
use stun::checks::*;
use stun::message::*;

/// `ReservationToken` represents `RESERVATION-TOKEN` attribute.
///
/// The `RESERVATION-TOKEN` attribute contains a token that uniquely
/// identifies a relayed transport address being held in reserve by the
/// server. The server includes this attribute in a success response to
/// tell the client about the token, and the client includes this
/// attribute in a subsequent Allocate request to request the server use
/// that relayed transport address for the allocation.
///
/// [RFC 5766 Section 14.9](https://www.rfc-editor.org/rfc/rfc5766#section-14.9).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReservationToken(pub Vec<u8>);

const RESERVATION_TOKEN_SIZE: usize = 8; // 8 bytes

impl Setter for ReservationToken {
    /// Adds `RESERVATION-TOKEN` to message.
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        check_size(ATTR_RESERVATION_TOKEN, self.0.len(), RESERVATION_TOKEN_SIZE)?;
        m.add(ATTR_RESERVATION_TOKEN, &self.0);
        Ok(())
    }
}

impl Getter for ReservationToken {
    /// Decodes `RESERVATION-TOKEN` from message.
    fn get_from(&mut self, m: &Message) -> Result<(), stun::Error> {
        let v = m.get(ATTR_RESERVATION_TOKEN)?;
        check_size(ATTR_RESERVATION_TOKEN, v.len(), RESERVATION_TOKEN_SIZE)?;
        self.0 = v;
        Ok(())
    }
}
