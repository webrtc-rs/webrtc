#[cfg(test)]
mod reqfamily_test;

use stun::attributes::*;
use stun::checks::*;
use stun::message::*;

use std::fmt;

// Values for RequestedAddressFamily as defined in RFC 6156 Section 4.1.1.
pub const REQUESTED_FAMILY_IPV4: RequestedAddressFamily = RequestedAddressFamily(0x01);
pub const REQUESTED_FAMILY_IPV6: RequestedAddressFamily = RequestedAddressFamily(0x02);

// RequestedAddressFamily represents the REQUESTED-ADDRESS-FAMILY Attribute as
// defined in RFC 6156 Section 4.1.1.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RequestedAddressFamily(pub u8);

impl fmt::Display for RequestedAddressFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            REQUESTED_FAMILY_IPV4 => "IPv4",
            REQUESTED_FAMILY_IPV6 => "IPv6",
            _ => "unknown",
        };
        write!(f, "{s}")
    }
}

const REQUESTED_FAMILY_SIZE: usize = 4;

impl Setter for RequestedAddressFamily {
    // AddTo adds REQUESTED-ADDRESS-FAMILY to message.
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        let mut v = vec![0; REQUESTED_FAMILY_SIZE];
        v[0] = self.0;
        // b[1:4] is RFFU = 0.
        // The RFFU field MUST be set to zero on transmission and MUST be
        // ignored on reception. It is reserved for future uses.
        m.add(ATTR_REQUESTED_ADDRESS_FAMILY, &v);
        Ok(())
    }
}

impl Getter for RequestedAddressFamily {
    // GetFrom decodes REQUESTED-ADDRESS-FAMILY from message.
    fn get_from(&mut self, m: &Message) -> Result<(), stun::Error> {
        let v = m.get(ATTR_REQUESTED_ADDRESS_FAMILY)?;
        check_size(
            ATTR_REQUESTED_ADDRESS_FAMILY,
            v.len(),
            REQUESTED_FAMILY_SIZE,
        )?;

        if v[0] != REQUESTED_FAMILY_IPV4.0 && v[0] != REQUESTED_FAMILY_IPV6.0 {
            return Err(stun::Error::Other("ErrInvalidRequestedFamilyValue".into()));
        }
        self.0 = v[0];
        Ok(())
    }
}
