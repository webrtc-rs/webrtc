use super::*;
use crate::message::packer::*;

// An AAAAResource is an aaaa Resource record.
#[derive(Default)]
pub struct AAAAResource {
    pub aaaa: [u8; 16],
}

impl fmt::Display for AAAAResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dnsmessage.AAAAResource{{aaaa: {:?}}}", self.aaaa)
    }
}

impl ResourceBody for AAAAResource {
    fn real_type(&self) -> DNSType {
        DNSType::AAAA
    }

    // pack appends the wire format of the AAAAResource to msg.
    fn pack(
        &self,
        msg: Vec<u8>,
        _compression: &mut Option<HashMap<String, usize>>,
        _compression_off: usize,
    ) -> Result<Vec<u8>, Error> {
        Ok(pack_bytes(msg, &self.aaaa))
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize, Error> {
        unpack_bytes(msg, off, &mut self.aaaa)
    }
}
