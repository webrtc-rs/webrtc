use super::*;
use crate::message::name::*;
use crate::message::packer::*;

// An MXResource is an mx Resource record.
#[derive(Default)]
pub struct MXResource {
    pref: u16,
    mx: Name,
}

impl fmt::Display for MXResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.MXResource{{pref: {}, mx: {}}}",
            self.pref, self.mx
        )
    }
}

impl ResourceBody for MXResource {
    fn real_type(&self) -> DNSType {
        DNSType::MX
    }

    // pack appends the wire format of the MXResource to msg.
    fn pack(
        &self,
        mut msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>, Error> {
        msg = pack_uint16(msg, self.pref);
        msg = self.mx.pack(msg, compression, compression_off)?;
        Ok(msg)
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize, Error> {
        let (pref, off) = unpack_uint16(msg, off)?;
        self.pref = pref;
        self.mx.unpack(msg, off)
    }
}
