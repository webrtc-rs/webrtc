use super::*;
use crate::message::packer::*;

// An AAAAResource is an aaaa Resource record.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct AaaaResource {
    pub aaaa: [u8; 16],
}

impl fmt::Display for AaaaResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dnsmessage.AAAAResource{{aaaa: {:?}}}", self.aaaa)
    }
}

impl ResourceBody for AaaaResource {
    fn real_type(&self) -> DnsType {
        DnsType::Aaaa
    }

    // pack appends the wire format of the AAAAResource to msg.
    fn pack(
        &self,
        msg: Vec<u8>,
        _compression: &mut Option<HashMap<String, usize>>,
        _compression_off: usize,
    ) -> Result<Vec<u8>> {
        Ok(pack_bytes(msg, &self.aaaa))
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize> {
        unpack_bytes(msg, off, &mut self.aaaa)
    }
}
