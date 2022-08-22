use super::*;
use crate::error::Result;
use crate::message::name::*;

// An NSResource is an NS Resource record.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct NsResource {
    pub ns: Name,
}

impl fmt::Display for NsResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dnsmessage.NSResource{{NS: {}}}", self.ns)
    }
}

impl ResourceBody for NsResource {
    fn real_type(&self) -> DnsType {
        DnsType::Ns
    }

    // pack appends the wire format of the NSResource to msg.
    fn pack(
        &self,
        msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>> {
        self.ns.pack(msg, compression, compression_off)
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _txt_length: usize) -> Result<usize> {
        self.ns.unpack(msg, off)
    }
}
