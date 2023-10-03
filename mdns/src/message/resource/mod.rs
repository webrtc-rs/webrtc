pub mod a;
pub mod aaaa;
pub mod cname;
pub mod mx;
pub mod ns;
pub mod opt;
pub mod ptr;
pub mod soa;
pub mod srv;
pub mod txt;

use std::collections::HashMap;
use std::fmt;

use a::*;
use aaaa::*;
use cname::*;
use mx::*;
use ns::*;
use opt::*;
use ptr::*;
use soa::*;
use srv::*;
use txt::*;

use super::name::*;
use super::packer::*;
use super::*;
use crate::error::*;

// EDNS(0) wire constants.

const EDNS0_VERSION: u32 = 0;
const EDNS0_DNSSEC_OK: u32 = 0x00008000;
const EDNS_VERSION_MASK: u32 = 0x00ff0000;
const EDNS0_DNSSEC_OK_MASK: u32 = 0x00ff8000;

// A Resource is a DNS resource record.
#[derive(Default, Debug)]
pub struct Resource {
    pub header: ResourceHeader,
    pub body: Option<Box<dyn ResourceBody>>,
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.Resource{{Header: {}, Body: {}}}",
            self.header,
            if let Some(body) = &self.body {
                body.to_string()
            } else {
                "None".to_owned()
            }
        )
    }
}

impl Resource {
    // pack appends the wire format of the Resource to msg.
    pub fn pack(
        &mut self,
        msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>> {
        if let Some(body) = &self.body {
            self.header.typ = body.real_type();
        } else {
            return Err(Error::ErrNilResourceBody);
        }
        let (mut msg, len_off) = self.header.pack(msg, compression, compression_off)?;
        let pre_len = msg.len();
        if let Some(body) = &self.body {
            msg = body.pack(msg, compression, compression_off)?;
            self.header.fix_len(&mut msg, len_off, pre_len)?;
        }
        Ok(msg)
    }

    pub fn unpack(&mut self, msg: &[u8], mut off: usize) -> Result<usize> {
        off = self.header.unpack(msg, off, 0)?;
        let (rb, off) =
            unpack_resource_body(self.header.typ, msg, off, self.header.length as usize)?;
        self.body = Some(rb);
        Ok(off)
    }

    pub(crate) fn skip(msg: &[u8], off: usize) -> Result<usize> {
        let mut new_off = Name::skip(msg, off)?;
        new_off = DnsType::skip(msg, new_off)?;
        new_off = DnsClass::skip(msg, new_off)?;
        new_off = skip_uint32(msg, new_off)?;
        let (length, mut new_off) = unpack_uint16(msg, new_off)?;
        new_off += length as usize;
        if new_off > msg.len() {
            return Err(Error::ErrResourceLen);
        }
        Ok(new_off)
    }
}

// A ResourceHeader is the header of a DNS resource record. There are
// many types of DNS resource records, but they all share the same header.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct ResourceHeader {
    // Name is the domain name for which this resource record pertains.
    pub name: Name,

    // Type is the type of DNS resource record.
    //
    // This field will be set automatically during packing.
    pub typ: DnsType,

    // Class is the class of network to which this DNS resource record
    // pertains.
    pub class: DnsClass,

    // TTL is the length of time (measured in seconds) which this resource
    // record is valid for (time to live). All Resources in a set should
    // have the same TTL (RFC 2181 Section 5.2).
    pub ttl: u32,

    // Length is the length of data in the resource record after the header.
    //
    // This field will be set automatically during packing.
    pub length: u16,
}

impl fmt::Display for ResourceHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.ResourceHeader{{Name: {}, Type: {}, Class: {}, TTL: {}, Length: {}}}",
            self.name, self.typ, self.class, self.ttl, self.length,
        )
    }
}

impl ResourceHeader {
    // pack appends the wire format of the ResourceHeader to oldMsg.
    //
    // lenOff is the offset in msg where the Length field was packed.
    pub fn pack(
        &self,
        mut msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<(Vec<u8>, usize)> {
        msg = self.name.pack(msg, compression, compression_off)?;
        msg = self.typ.pack(msg);
        msg = self.class.pack(msg);
        msg = pack_uint32(msg, self.ttl);
        let len_off = msg.len();
        msg = pack_uint16(msg, self.length);
        Ok((msg, len_off))
    }

    pub fn unpack(&mut self, msg: &[u8], off: usize, _length: usize) -> Result<usize> {
        let mut new_off = off;
        new_off = self.name.unpack(msg, new_off)?;
        new_off = self.typ.unpack(msg, new_off)?;
        new_off = self.class.unpack(msg, new_off)?;
        let (ttl, new_off) = unpack_uint32(msg, new_off)?;
        self.ttl = ttl;
        let (l, new_off) = unpack_uint16(msg, new_off)?;
        self.length = l;

        Ok(new_off)
    }

    // fixLen updates a packed ResourceHeader to include the length of the
    // ResourceBody.
    //
    // lenOff is the offset of the ResourceHeader.Length field in msg.
    //
    // preLen is the length that msg was before the ResourceBody was packed.
    pub fn fix_len(&mut self, msg: &mut [u8], len_off: usize, pre_len: usize) -> Result<()> {
        if msg.len() < pre_len || msg.len() > pre_len + u16::MAX as usize {
            return Err(Error::ErrResTooLong);
        }

        let con_len = msg.len() - pre_len;

        // Fill in the length now that we know how long the content is.
        msg[len_off] = ((con_len >> 8) & 0xFF) as u8;
        msg[len_off + 1] = (con_len & 0xFF) as u8;
        self.length = con_len as u16;

        Ok(())
    }

    // set_edns0 configures h for EDNS(0).
    //
    // The provided ext_rcode must be an extended RCode.
    pub fn set_edns0(
        &mut self,
        udp_payload_len: u16,
        ext_rcode: u32,
        dnssec_ok: bool,
    ) -> Result<()> {
        self.name = Name {
            data: ".".to_owned(),
        }; // RFC 6891 section 6.1.2
        self.typ = DnsType::Opt;
        self.class = DnsClass(udp_payload_len);
        self.ttl = (ext_rcode >> 4) << 24;
        if dnssec_ok {
            self.ttl |= EDNS0_DNSSEC_OK;
        }
        Ok(())
    }

    // dnssec_allowed reports whether the DNSSEC OK bit is set.
    pub fn dnssec_allowed(&self) -> bool {
        self.ttl & EDNS0_DNSSEC_OK_MASK == EDNS0_DNSSEC_OK // RFC 6891 section 6.1.3
    }

    // extended_rcode returns an extended RCode.
    //
    // The provided rcode must be the RCode in DNS message header.
    pub fn extended_rcode(&self, rcode: RCode) -> RCode {
        if self.ttl & EDNS_VERSION_MASK == EDNS0_VERSION {
            // RFC 6891 section 6.1.3
            let ttl = ((self.ttl >> 24) << 4) as u8 | rcode as u8;
            return RCode::from(ttl);
        }
        rcode
    }
}

// A ResourceBody is a DNS resource record minus the header.
pub trait ResourceBody: fmt::Display + fmt::Debug {
    // real_type returns the actual type of the Resource. This is used to
    // fill in the header Type field.
    fn real_type(&self) -> DnsType;

    // pack packs a Resource except for its header.
    fn pack(
        &self,
        msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>>;

    fn unpack(&mut self, msg: &[u8], off: usize, length: usize) -> Result<usize>;
}

pub fn unpack_resource_body(
    typ: DnsType,
    msg: &[u8],
    mut off: usize,
    length: usize,
) -> Result<(Box<dyn ResourceBody>, usize)> {
    let mut rb: Box<dyn ResourceBody> = match typ {
        DnsType::A => Box::<AResource>::default(),
        DnsType::Ns => Box::<NsResource>::default(),
        DnsType::Cname => Box::<CnameResource>::default(),
        DnsType::Soa => Box::<SoaResource>::default(),
        DnsType::Ptr => Box::<PtrResource>::default(),
        DnsType::Mx => Box::<MxResource>::default(),
        DnsType::Txt => Box::<TxtResource>::default(),
        DnsType::Aaaa => Box::<AaaaResource>::default(),
        DnsType::Srv => Box::<SrvResource>::default(),
        DnsType::Opt => Box::<OptResource>::default(),
        _ => return Err(Error::ErrNilResourceBody),
    };

    off = rb.unpack(msg, off, length)?;

    Ok((rb, off))
}
