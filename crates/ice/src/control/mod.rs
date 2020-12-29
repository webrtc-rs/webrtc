#[cfg(test)]
mod control_test;

use stun::attributes::*;
use stun::checks::*;
use stun::errors::ERR_ATTRIBUTE_NOT_FOUND;
use stun::message::*;

use util::Error;

use std::fmt;

// Tiebreaker is common helper for ICE-{CONTROLLED,CONTROLLING}
// and represents the so-called Tiebreaker number.
#[derive(Default, PartialEq, Debug, Copy, Clone)]
pub struct TieBreaker(pub u64);

pub(crate) const TIE_BREAKER_SIZE: usize = 8; // 64 bit

impl TieBreaker {
    // add_to_as adds Tiebreaker value to m as t attribute.
    pub fn add_to_as(&self, m: &mut Message, t: AttrType) -> Result<(), Error> {
        let mut v = vec![0; TIE_BREAKER_SIZE];
        v.copy_from_slice(&(self.0 as u64).to_be_bytes());
        m.add(t, &v);
        Ok(())
    }

    // get_from_as decodes Tiebreaker value in message getting it as for t type.
    pub fn get_from_as(&mut self, m: &Message, t: AttrType) -> Result<(), Error> {
        let v = m.get(t)?;
        check_size(t, v.len(), TIE_BREAKER_SIZE)?;
        self.0 = u64::from_be_bytes([v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]]);
        Ok(())
    }
}
// AttrControlled represents ICE-CONTROLLED attribute.
#[derive(Default, PartialEq, Debug, Copy, Clone)]
pub struct AttrControlled(pub u64);

impl Setter for AttrControlled {
    // add_to adds ICE-CONTROLLED to message.
    fn add_to(&self, m: &mut Message) -> Result<(), Error> {
        TieBreaker(self.0).add_to_as(m, ATTR_ICE_CONTROLLED)
    }
}

impl Getter for AttrControlled {
    // get_from decodes ICE-CONTROLLED from message.
    fn get_from(&mut self, m: &Message) -> Result<(), Error> {
        let mut t = TieBreaker::default();
        t.get_from_as(m, ATTR_ICE_CONTROLLED)?;
        self.0 = t.0;
        Ok(())
    }
}

// AttrControlling represents ICE-CONTROLLING attribute.
#[derive(Default, PartialEq, Debug, Copy, Clone)]
pub struct AttrControlling(pub u64);

impl Setter for AttrControlling {
    // add_to adds ICE-CONTROLLING to message.
    fn add_to(&self, m: &mut Message) -> Result<(), Error> {
        TieBreaker(self.0).add_to_as(m, ATTR_ICE_CONTROLLING)
    }
}

impl Getter for AttrControlling {
    // get_from decodes ICE-CONTROLLING from message.
    fn get_from(&mut self, m: &Message) -> Result<(), Error> {
        let mut t = TieBreaker::default();
        t.get_from_as(m, ATTR_ICE_CONTROLLING)?;
        self.0 = t.0;
        Ok(())
    }
}

// AttrControl is helper that wraps ICE-{CONTROLLED,CONTROLLING}.
#[derive(Default, PartialEq, Debug, Copy, Clone)]
pub struct AttrControl {
    role: Role,
    tie_breaker: TieBreaker,
}

impl Setter for AttrControl {
    // add_to adds ICE-CONTROLLED or ICE-CONTROLLING attribute depending on Role.
    fn add_to(&self, m: &mut Message) -> Result<(), Error> {
        if self.role == Role::Controlling {
            self.tie_breaker.add_to_as(m, ATTR_ICE_CONTROLLING)
        } else {
            self.tie_breaker.add_to_as(m, ATTR_ICE_CONTROLLED)
        }
    }
}

impl Getter for AttrControl {
    // get_from decodes Role and Tiebreaker value from message.
    fn get_from(&mut self, m: &Message) -> Result<(), Error> {
        if m.contains(ATTR_ICE_CONTROLLING) {
            self.role = Role::Controlling;
            return self.tie_breaker.get_from_as(m, ATTR_ICE_CONTROLLING);
        }
        if m.contains(ATTR_ICE_CONTROLLED) {
            self.role = Role::Controlled;
            return self.tie_breaker.get_from_as(m, ATTR_ICE_CONTROLLED);
        }

        Err(ERR_ATTRIBUTE_NOT_FOUND.to_owned())
    }
}

// Role represents ICE agent role, which can be controlling or controlled.
// Possible ICE agent roles.
#[derive(PartialEq, Copy, Clone, Debug)]
pub enum Role {
    Controlling,
    Controlled,
    Unspecified,
}

impl Default for Role {
    fn default() -> Self {
        Role::Controlling
    }
}

impl From<&str> for Role {
    fn from(raw: &str) -> Self {
        match raw {
            "controlling" => Role::Controlling,
            "controlled" => Role::Controlled,
            _ => Role::Unspecified,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Role::Controlling => "controlling",
            Role::Controlled => "controlled",
            Role::Unspecified => "unspecified",
        };
        write!(f, "{}", s)
    }
}
