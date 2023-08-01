use std::collections::HashMap;
use std::fmt;

use super::name::*;
use super::*;
use crate::error::Result;

// A question is a DNS query.
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Question {
    pub name: Name,
    pub typ: DnsType,
    pub class: DnsClass,
}

impl fmt::Display for Question {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.question{{Name: {}, Type: {}, Class: {}}}",
            self.name, self.typ, self.class
        )
    }
}

impl Question {
    // pack appends the wire format of the question to msg.
    pub fn pack(
        &self,
        mut msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>> {
        msg = self.name.pack(msg, compression, compression_off)?;
        msg = self.typ.pack(msg);
        Ok(self.class.pack(msg))
    }
}
