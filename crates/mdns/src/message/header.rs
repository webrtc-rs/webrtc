use super::*;

// Header is a representation of a DNS message header.
pub struct Header {
    id: u16,
    response: bool,
    op_code: OpCode,
    authoritative: bool,
    truncated: bool,
    recursion_desired: bool,
    recursion_available: bool,
    rcode: RCode,
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dnsmessage.Header{{id: {}, response: {}, op_code: {}, authoritative: {}, truncated: {}, recursion_desired: {}, recursion_available: {}, rcode: {} }}",
            self.id,
            self.response,
            self.op_code,
            self.authoritative,
            self.truncated,
            self.recursion_desired,
            self.recursion_available,
            self.rcode
        )
    }
}

impl Header {
    pub fn pack(&self) -> (u16, u16) {
        let id = self.id;
        let mut bits = (self.op_code as u16) << 11 | self.rcode as u16;
        if self.recursion_available {
            bits |= HEADER_BIT_RA
        }
        if self.recursion_desired {
            bits |= HEADER_BIT_RD
        }
        if self.truncated {
            bits |= HEADER_BIT_TC
        }
        if self.authoritative {
            bits |= HEADER_BIT_AA
        }
        if self.response {
            bits |= HEADER_BIT_QR
        }

        (id, bits)
    }
}
