#[cfg(test)]
mod lifetime_test;

use std::fmt;
use std::time::Duration;

use stun::attributes::*;
use stun::checks::*;
use stun::message::*;

/// `DEFAULT_LIFETIME` in RFC 5766 is 10 minutes.
///
/// [RFC 5766 Section 2.2](https://www.rfc-editor.org/rfc/rfc5766#section-2.2).
pub const DEFAULT_LIFETIME: Duration = Duration::from_secs(10 * 60);

/// `Lifetime` represents `LIFETIME` attribute.
///
/// The `LIFETIME` attribute represents the duration for which the server
/// will maintain an allocation in the absence of a refresh. The value
/// portion of this attribute is 4-bytes long and consists of a 32-bit
/// unsigned integral value representing the number of seconds remaining
/// until expiration.
///
/// [RFC 5766 Section 14.2](https://www.rfc-editor.org/rfc/rfc5766#section-14.2).
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Lifetime(pub Duration);

impl fmt::Display for Lifetime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.0.as_secs())
    }
}

// uint32 seconds
const LIFETIME_SIZE: usize = 4; // 4 bytes, 32 bits

impl Setter for Lifetime {
    /// Adds `LIFETIME` to message.
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        let mut v = vec![0; LIFETIME_SIZE];
        v.copy_from_slice(&(self.0.as_secs() as u32).to_be_bytes());
        m.add(ATTR_LIFETIME, &v);
        Ok(())
    }
}

impl Getter for Lifetime {
    /// Decodes `LIFETIME` from message.
    fn get_from(&mut self, m: &Message) -> Result<(), stun::Error> {
        let v = m.get(ATTR_LIFETIME)?;

        check_size(ATTR_LIFETIME, v.len(), LIFETIME_SIZE)?;

        let seconds = u32::from_be_bytes([v[0], v[1], v[2], v[3]]);
        self.0 = Duration::from_secs(seconds as u64);

        Ok(())
    }
}
