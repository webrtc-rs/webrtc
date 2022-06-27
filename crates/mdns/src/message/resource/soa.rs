use super::*;
use crate::error::Result;
use crate::message::name::*;
use crate::message::packer::*;

// An SOAResource is an SOA Resource record.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct SoaResource {
    pub ns: Name,
    pub mbox: Name,
    pub serial: u32,
    pub refresh: u32,
    pub retry: u32,
    pub expire: u32,

    // min_ttl the is the default TTL of Resources records which did not
    // contain a TTL value and the TTL of negative responses. (RFC 2308
    // Section 4)
    pub min_ttl: u32,
}

impl fmt::Display for SoaResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.SOAResource{{ns: {}, mbox: {}, serial: {}, refresh: {}, retry: {}, expire: {}, min_ttl: {}}}",
            self.ns,
            self.mbox,
            self.serial,
            self.refresh,
            self.retry,
            self.expire,
            self.min_ttl,
        )
    }
}

impl ResourceBody for SoaResource {
    fn real_type(&self) -> DnsType {
        DnsType::Soa
    }

    // pack appends the wire format of the SOAResource to msg.
    fn pack(
        &self,
        mut msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>> {
        msg = self.ns.pack(msg, compression, compression_off)?;
        msg = self.mbox.pack(msg, compression, compression_off)?;
        msg = pack_uint32(msg, self.serial);
        msg = pack_uint32(msg, self.refresh);
        msg = pack_uint32(msg, self.retry);
        msg = pack_uint32(msg, self.expire);
        Ok(pack_uint32(msg, self.min_ttl))
    }

    fn unpack(&mut self, msg: &[u8], mut off: usize, _length: usize) -> Result<usize> {
        off = self.ns.unpack(msg, off)?;
        off = self.mbox.unpack(msg, off)?;

        let (serial, off) = unpack_uint32(msg, off)?;
        self.serial = serial;

        let (refresh, off) = unpack_uint32(msg, off)?;
        self.refresh = refresh;

        let (retry, off) = unpack_uint32(msg, off)?;
        self.retry = retry;

        let (expire, off) = unpack_uint32(msg, off)?;
        self.expire = expire;

        let (min_ttl, off) = unpack_uint32(msg, off)?;
        self.min_ttl = min_ttl;

        Ok(off)
    }
}
