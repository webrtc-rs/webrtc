use super::*;
use crate::message::name::*;

// A cnameresource is a cname Resource record.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CnameResource {
    pub cname: Name,
}

impl fmt::Display for CnameResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dnsmessage.cnameresource{{cname: {}}}", self.cname)
    }
}

impl ResourceBody for CnameResource {
    fn real_type(&self) -> DnsType {
        DnsType::Cname
    }

    // pack appends the wire format of the cnameresource to msg.
    fn pack(
        &self,
        msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>> {
        self.cname.pack(msg, compression, compression_off)
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize> {
        self.cname.unpack(msg, off)
    }
}
