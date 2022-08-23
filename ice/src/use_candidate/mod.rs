#[cfg(test)]
mod use_candidate_test;

use stun::attributes::ATTR_USE_CANDIDATE;
use stun::message::*;

/// Represents USE-CANDIDATE attribute.
#[derive(Default)]
pub struct UseCandidateAttr;

impl Setter for UseCandidateAttr {
    /// Adds USE-CANDIDATE attribute to message.
    fn add_to(&self, m: &mut Message) -> Result<(), stun::Error> {
        m.add(ATTR_USE_CANDIDATE, &[]);
        Ok(())
    }
}

impl UseCandidateAttr {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns true if USE-CANDIDATE attribute is set.
    #[must_use]
    pub fn is_set(m: &Message) -> bool {
        let result = m.get(ATTR_USE_CANDIDATE);
        result.is_ok()
    }
}
