#[cfg(test)]
mod data_test;

use stun::attributes::*;
use stun::message::*;

/// `Data` represents `DATA` attribute.
///
/// The `DATA` attribute is present in all Send and Data indications.  The
/// value portion of this attribute is variable length and consists of
/// the application data (that is, the data that would immediately follow
/// the UDP header if the data was been sent directly between the client
/// and the peer).
///
/// [RFC 5766 Section 14.4](https://www.rfc-editor.org/rfc/rfc5766#section-14.4).
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Data(pub Vec<u8>);

impl Setter for Data {
    /// Adds `DATA` to message.
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        m.add(ATTR_DATA, &self.0);
        Ok(())
    }
}

impl Getter for Data {
    /// Decodes `DATA` from message.
    fn get_from(&mut self, m: &Message) -> Result<(), stun::Error> {
        self.0 = m.get(ATTR_DATA)?;
        Ok(())
    }
}
