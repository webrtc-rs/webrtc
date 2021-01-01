use super::*;
use crate::message::name::*;
use crate::message::packer::*;

// An SRVResource is an SRV Resource record.
pub struct SRVResource {
    priority: u16,
    weight: u16,
    port: u16,
    target: Name, // Not compressed as per RFC 2782.
}

impl fmt::Display for SRVResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.SRVResource{{priority: {}, weight: {}, port: {}, target: {}}}",
            self.priority, self.weight, self.port, self.target
        )
    }
}

impl ResourceBody for SRVResource {
    fn real_type(&self) -> DNSType {
        DNSType::SRV
    }

    // pack appends the wire format of the SRVResource to msg.
    fn pack(
        &self,
        mut msg: Vec<u8>,
        _compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>, Error> {
        msg = pack_uint16(msg, self.priority);
        msg = pack_uint16(msg, self.weight);
        msg = pack_uint16(msg, self.port);
        msg = self.target.pack(msg, &mut None, compression_off)?;
        Ok(msg)
    }

    fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize, Error> {
        let (priority, off) = unpack_uint16(msg, off)?;
        self.priority = priority;

        let (weight, off) = unpack_uint16(msg, off)?;
        self.weight = weight;

        let (port, off) = unpack_uint16(msg, off)?;
        self.port = port;

        self.target
            .unpack_compressed(msg, off, false /* allowCompression */)?;

        Ok(off)
    }
}
