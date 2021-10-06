#[cfg(test)]
mod priority_test;

use stun::attributes::ATTR_PRIORITY;
use stun::checks::*;
use stun::message::*;

/// Represents PRIORITY attribute.
#[derive(Default, PartialEq, Debug, Copy, Clone)]
pub struct PriorityAttr(pub u32);

const PRIORITY_SIZE: usize = 4; // 32 bit

impl Setter for PriorityAttr {
    // add_to adds PRIORITY attribute to message.
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        let mut v = vec![0_u8; PRIORITY_SIZE];
        v.copy_from_slice(&self.0.to_be_bytes());
        m.add(ATTR_PRIORITY, &v);
        Ok(())
    }
}

impl PriorityAttr {
    /// Decodes PRIORITY attribute from message.
    pub fn get_from(&mut self, m: &Message) -> Result<(), stun::Error> {
        let v = m.get(ATTR_PRIORITY)?;

        check_size(ATTR_PRIORITY, v.len(), PRIORITY_SIZE)?;

        let p = u32::from_be_bytes([v[0], v[1], v[2], v[3]]);
        self.0 = p;

        Ok(())
    }
}
