use super::*;
use crate::message::name::*;

// A cnameresource is a cname Resource record.
#[derive(Default)]
pub struct CNAMEResource {
    pub cname: Name,
}

impl fmt::Display for CNAMEResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dnsmessage.cnameresource{{cname: {}}}", self.cname)
    }
}

impl ResourceBody for CNAMEResource {
    fn real_type(&self) -> DNSType {
        DNSType::CNAME
    }

    // pack appends the wire format of the cnameresource to msg.
    fn pack(
        &self,
        msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>, Error> {
        self.cname.pack(msg, compression, compression_off)
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize, Error> {
        self.cname.unpack(msg, off)
    }
}
