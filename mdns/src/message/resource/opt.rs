use super::*;
use crate::error::{Result, *};
use crate::message::packer::*;

// An OPTResource is an OPT pseudo Resource record.
//
// The pseudo resource record is part of the extension mechanisms for DNS
// as defined in RFC 6891.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct OptResource {
    pub options: Vec<DnsOption>,
}

// An Option represents a DNS message option within OPTResource.
//
// The message option is part of the extension mechanisms for DNS as
// defined in RFC 6891.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct DnsOption {
    pub code: u16, // option code
    pub data: Vec<u8>,
}

impl fmt::Display for DnsOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.Option{{Code: {}, Data: {:?}}}",
            self.code, self.data
        )
    }
}

impl fmt::Display for OptResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s: Vec<String> = self.options.iter().map(|o| o.to_string()).collect();
        write!(f, "dnsmessage.OPTResource{{options: {}}}", s.join(","))
    }
}

impl ResourceBody for OptResource {
    fn real_type(&self) -> DnsType {
        DnsType::Opt
    }

    fn pack(
        &self,
        mut msg: Vec<u8>,
        _compression: &mut Option<HashMap<String, usize>>,
        _compression_off: usize,
    ) -> Result<Vec<u8>> {
        for opt in &self.options {
            msg = pack_uint16(msg, opt.code);
            msg = pack_uint16(msg, opt.data.len() as u16);
            msg = pack_bytes(msg, &opt.data);
        }
        Ok(msg)
    }

    fn unpack(&mut self, msg: &[u8], mut off: usize, length: usize) -> Result<usize> {
        let mut opts = vec![];
        let old_off = off;
        while off < old_off + length {
            let (code, new_off) = unpack_uint16(msg, off)?;
            off = new_off;

            let (l, new_off) = unpack_uint16(msg, off)?;
            off = new_off;

            let mut opt = DnsOption {
                code,
                data: vec![0; l as usize],
            };
            if off + l as usize > msg.len() {
                return Err(Error::ErrCalcLen);
            }
            opt.data.copy_from_slice(&msg[off..off + l as usize]);
            off += l as usize;
            opts.push(opt);
        }
        self.options = opts;
        Ok(off)
    }
}
