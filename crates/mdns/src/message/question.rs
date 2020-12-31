use super::name::*;
use super::packer::*;
use super::*;

use std::collections::HashMap;
use std::fmt;

use util::Error;

// A Question is a DNS query.
pub struct Question {
    pub name: Name,
    pub typ: DNSType,
    pub class: DNSClass,
}

impl fmt::Display for Question {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.Question{{Name: {}, Type: {}, Class: {}}}",
            self.name, self.typ, self.class
        )
    }
}

impl Question {
    // pack appends the wire format of the Question to msg.
    pub fn pack(
        &self,
        msg: &[u8],
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>, Error> {
        let mut msg = self.name.pack(msg, compression, compression_off)?;
        msg = pack_type(msg, self.typ);
        Ok(pack_class(msg, self.class))
    }
}
