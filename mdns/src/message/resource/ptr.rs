use super::*;
use crate::error::Result;
use crate::message::name::*;

// A PTRResource is a PTR Resource record.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct PtrResource {
    pub ptr: Name,
}

impl fmt::Display for PtrResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dnsmessage.PTRResource{{PTR: {}}}", self.ptr)
    }
}

impl ResourceBody for PtrResource {
    fn real_type(&self) -> DnsType {
        DnsType::Ptr
    }

    // pack appends the wire format of the PTRResource to msg.
    fn pack(
        &self,
        msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>> {
        self.ptr.pack(msg, compression, compression_off)
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize> {
        self.ptr.unpack(msg, off)
    }
}
