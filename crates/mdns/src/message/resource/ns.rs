use super::*;
use crate::message::name::*;

// An NSResource is an NS Resource record.
pub struct NSResource {
    ns: Name,
}

impl fmt::Display for NSResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dnsmessage.NSResource{{NS: {}}}", self.ns)
    }
}

impl ResourceBody for NSResource {
    fn real_type(&self) -> DNSType {
        DNSType::NS
    }

    // pack appends the wire format of the NSResource to msg.
    fn pack(
        &self,
        msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>, Error> {
        self.ns.pack(msg, compression, compression_off)
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _txt_length: usize) -> Result<usize, Error> {
        self.ns.unpack(msg, off)
    }
}
