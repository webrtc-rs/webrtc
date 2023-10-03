#[cfg(test)]
mod channnum_test;

use std::fmt;

use stun::attributes::*;
use stun::checks::*;
use stun::message::*;

// 16 bits of uint + 16 bits of RFFU = 0.
const CHANNEL_NUMBER_SIZE: usize = 4;

// See https://tools.ietf.org/html/rfc5766#section-11:
//
// 0x4000 through 0x7FFF: These values are the allowed channel
// numbers (16,383 possible values).
pub const MIN_CHANNEL_NUMBER: u16 = 0x4000;
pub const MAX_CHANNEL_NUMBER: u16 = 0x7FFF;

/// `ChannelNumber` represents `CHANNEL-NUMBER` attribute. Encoded as `u16`.
///
/// The `CHANNEL-NUMBER` attribute contains the number of the channel.
///
/// [RFC 5766 Section 14.1](https://www.rfc-editor.org/rfc/rfc5766#section-14.1).
#[derive(Default, Eq, PartialEq, Debug, Copy, Clone, Hash)]
pub struct ChannelNumber(pub u16);

impl fmt::Display for ChannelNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Setter for ChannelNumber {
    /// Adds `CHANNEL-NUMBER` to message.
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        let mut v = vec![0; CHANNEL_NUMBER_SIZE];
        v[..2].copy_from_slice(&self.0.to_be_bytes());
        // v[2:4] are zeroes (RFFU = 0)
        m.add(ATTR_CHANNEL_NUMBER, &v);
        Ok(())
    }
}

impl Getter for ChannelNumber {
    /// Decodes `CHANNEL-NUMBER` from message.
    fn get_from(&mut self, m: &Message) -> Result<(), stun::Error> {
        let v = m.get(ATTR_CHANNEL_NUMBER)?;

        check_size(ATTR_CHANNEL_NUMBER, v.len(), CHANNEL_NUMBER_SIZE)?;

        //_ = v[CHANNEL_NUMBER_SIZE-1] // asserting length
        self.0 = u16::from_be_bytes([v[0], v[1]]);
        // v[2:4] is RFFU and equals to 0.
        Ok(())
    }
}

impl ChannelNumber {
    /// Returns true if c in `[0x4000, 0x7FFF]`.
    fn is_channel_number_valid(&self) -> bool {
        self.0 >= MIN_CHANNEL_NUMBER && self.0 <= MAX_CHANNEL_NUMBER
    }

    /// returns `true` if channel number has correct value that complies
    /// [RFC 5766 Section 11](https://www.rfc-editor.org/rfc/rfc5766#section-11) range.
    pub fn valid(&self) -> bool {
        self.is_channel_number_valid()
    }
}
