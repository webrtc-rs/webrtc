#[cfg(test)]
mod use_candidate_test;

use stun::attributes::ATTR_USE_CANDIDATE;
use stun::message::*;

use util::Error;

// UseCandidateAttr represents USE-CANDIDATE attribute.
#[derive(Default)]
pub struct UseCandidateAttr;

impl Setter for UseCandidateAttr {
    // add_to adds USE-CANDIDATE attribute to message.
    fn add_to(&self, m: &mut Message) -> Result<(), Error> {
        m.add(ATTR_USE_CANDIDATE, &[]);
        Ok(())
    }
}

impl UseCandidateAttr {
    // UseCandidate is shorthand for UseCandidateAttr.
    pub fn new() -> Self {
        UseCandidateAttr {}
    }

    // IsSet returns true if USE-CANDIDATE attribute is set.
    pub fn is_set(m: &Message) -> bool {
        let result = m.get(ATTR_USE_CANDIDATE);
        result.is_ok()
    }
}
