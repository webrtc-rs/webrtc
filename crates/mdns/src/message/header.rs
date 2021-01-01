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

/*


// header is the wire format for a DNS message header.
type header struct {
    id          uint16
    bits        uint16
    questions   uint16
    answers     uint16
    authorities uint16
    additionals uint16
}

func (h *header) count(sec section) uint16 {
    switch sec {
    case sectionQuestions:
        return h.questions
    case sectionAnswers:
        return h.answers
    case sectionAuthorities:
        return h.authorities
    case sectionAdditionals:
        return h.additionals
    }
    return 0
}

// pack appends the wire format of the header to msg.
func (h *header) pack(msg []byte) []byte {
    msg = pack_uint16(msg, h.id)
    msg = pack_uint16(msg, h.bits)
    msg = pack_uint16(msg, h.questions)
    msg = pack_uint16(msg, h.answers)
    msg = pack_uint16(msg, h.authorities)
    return pack_uint16(msg, h.additionals)
}

func (h *header) unpack(msg []byte, off int) (int, error) {
    newOff := off
    var err error
    if h.id, newOff, err = unpack_uint16(msg, newOff); err != nil {
        return off, &nestedError{"id", err}
    }
    if h.bits, newOff, err = unpack_uint16(msg, newOff); err != nil {
        return off, &nestedError{"bits", err}
    }
    if h.questions, newOff, err = unpack_uint16(msg, newOff); err != nil {
        return off, &nestedError{"questions", err}
    }
    if h.answers, newOff, err = unpack_uint16(msg, newOff); err != nil {
        return off, &nestedError{"answers", err}
    }
    if h.authorities, newOff, err = unpack_uint16(msg, newOff); err != nil {
        return off, &nestedError{"authorities", err}
    }
    if h.additionals, newOff, err = unpack_uint16(msg, newOff); err != nil {
        return off, &nestedError{"additionals", err}
    }
    return newOff, nil
}

func (h *header) header() Header {
    return Header{
        ID:                 h.id,
        Response:           (h.bits & HEADER_BIT_QR) != 0,
        OpCode:             OpCode(h.bits>>11) & 0xF,
        Authoritative:      (h.bits & HEADER_BIT_AA) != 0,
        Truncated:          (h.bits & HEADER_BIT_TC) != 0,
        RecursionDesired:   (h.bits & HEADER_BIT_RD) != 0,
        RecursionAvailable: (h.bits & HEADER_BIT_RA) != 0,
        RCode:              RCode(h.bits & 0xF),
    }
}

 */
