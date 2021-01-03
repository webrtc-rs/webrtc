use super::*;
use crate::errors::*;
use crate::message::packer::*;

// A TXTResource is a txt Resource record.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct TXTResource {
    pub txt: Vec<String>,
}

impl fmt::Display for TXTResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.txt.is_empty() {
            write!(f, "dnsmessage.TXTResource{{txt: {{}}}}",)
        } else {
            write!(f, "dnsmessage.TXTResource{{txt: {{{}}}", self.txt.join(","))
        }
    }
}

impl ResourceBody for TXTResource {
    fn real_type(&self) -> DNSType {
        DNSType::TXT
    }

    // pack appends the wire format of the TXTResource to msg.
    fn pack(
        &self,
        mut msg: Vec<u8>,
        _compression: &mut Option<HashMap<String, usize>>,
        _compression_off: usize,
    ) -> Result<Vec<u8>, Error> {
        for s in &self.txt {
            msg = pack_str(msg, s)?
        }
        Ok(msg)
    }

    fn unpack(&mut self, msg: &[u8], mut off: usize, length: usize) -> Result<usize, Error> {
        let mut txts = vec![];
        let mut n = 0;
        while n < length {
            let (t, new_off) = unpack_str(msg, off)?;
            off = new_off;
            // Check if we got too many bytes.
            if length < n + t.as_bytes().len() + 1 {
                return Err(ERR_CALC_LEN.to_owned());
            }
            n += t.len() + 1;
            txts.push(t);
        }
        self.txt = txts;

        Ok(off)
    }
}
