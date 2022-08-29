use super::*;
use crate::error::Result;
use crate::message::name::*;
use crate::message::packer::*;

// An MXResource is an mx Resource record.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct MxResource {
    pub pref: u16,
    pub mx: Name,
}

impl fmt::Display for MxResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.MXResource{{pref: {}, mx: {}}}",
            self.pref, self.mx
        )
    }
}

impl ResourceBody for MxResource {
    fn real_type(&self) -> DnsType {
        DnsType::Mx
    }

    // pack appends the wire format of the MXResource to msg.
    fn pack(
        &self,
        mut msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>> {
        msg = pack_uint16(msg, self.pref);
        msg = self.mx.pack(msg, compression, compression_off)?;
        Ok(msg)
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize> {
        let (pref, off) = unpack_uint16(msg, off)?;
        self.pref = pref;
        self.mx.unpack(msg, off)
    }
}
