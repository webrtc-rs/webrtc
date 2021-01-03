use super::name::*;
use super::*;

use std::collections::HashMap;
use std::fmt;

use util::Error;

// A question is a DNS query.
#[derive(Default, Debug, PartialEq, Clone)]
pub struct Question {
    pub name: Name,
    pub typ: DNSType,
    pub class: DNSClass,
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
    ) -> Result<Vec<u8>, Error> {
        msg = self.name.pack(msg, compression, compression_off)?;
        msg = self.typ.pack(msg);
        Ok(self.class.pack(msg))
    }
}
