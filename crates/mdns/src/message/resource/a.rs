use super::*;
use crate::message::packer::*;

// An AResource is an A Resource record.
#[derive(Default)]
pub struct AResource {
    a: [u8; 4],
}

impl fmt::Display for AResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dnsmessage.AResource{{A: {:?}}}", self.a)
    }
}

impl ResourceBody for AResource {
    fn real_type(&self) -> DNSType {
        DNSType::A
    }

    // pack appends the wire format of the AResource to msg.
    fn pack(
        &self,
        msg: Vec<u8>,
        _compression: &mut Option<HashMap<String, usize>>,
        _compression_off: usize,
    ) -> Result<Vec<u8>, Error> {
        Ok(pack_bytes(msg, &self.a))
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize, Error> {
        unpack_bytes(msg, off, &mut self.a)
    }
}
