use super::*;

// Header is a representation of a DNS message header.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Header {
    pub id: u16,
    pub response: bool,
    pub op_code: OpCode,
    pub authoritative: bool,
    pub truncated: bool,
    pub recursion_desired: bool,
    pub recursion_available: bool,
    pub rcode: RCode,
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
        let mut bits = self.op_code << 11 | self.rcode as u16;
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

#[derive(Default, Copy, Clone, PartialOrd, PartialEq, Eq)]
pub enum Section {
    #[default]
    NotStarted = 0,
    Header = 1,
    Questions = 2,
    Answers = 3,
    Authorities = 4,
    Additionals = 5,
    Done = 6,
}

impl From<u8> for Section {
    fn from(v: u8) -> Self {
        match v {
            0 => Section::NotStarted,
            1 => Section::Header,
            2 => Section::Questions,
            3 => Section::Answers,
            4 => Section::Authorities,
            5 => Section::Additionals,
            _ => Section::Done,
        }
    }
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Section::NotStarted => "NotStarted",
            Section::Header => "Header",
            Section::Questions => "question",
            Section::Answers => "answer",
            Section::Authorities => "authority",
            Section::Additionals => "additional",
            Section::Done => "Done",
        };
        write!(f, "{s}")
    }
}

// header is the wire format for a DNS message header.
#[derive(Default)]
pub struct HeaderInternal {
    pub id: u16,
    pub bits: u16,
    pub questions: u16,
    pub answers: u16,
    pub authorities: u16,
    pub additionals: u16,
}

impl HeaderInternal {
    pub(crate) fn count(&self, sec: Section) -> u16 {
        match sec {
            Section::Questions => self.questions,
            Section::Answers => self.answers,
            Section::Authorities => self.authorities,
            Section::Additionals => self.additionals,
            _ => 0,
        }
    }

    // pack appends the wire format of the header to msg.
    pub(crate) fn pack(&self, mut msg: Vec<u8>) -> Vec<u8> {
        msg = pack_uint16(msg, self.id);
        msg = pack_uint16(msg, self.bits);
        msg = pack_uint16(msg, self.questions);
        msg = pack_uint16(msg, self.answers);
        msg = pack_uint16(msg, self.authorities);
        msg = pack_uint16(msg, self.additionals);
        msg
    }

    pub(crate) fn unpack(&mut self, msg: &[u8], off: usize) -> Result<usize> {
        let (id, off) = unpack_uint16(msg, off)?;
        self.id = id;

        let (bits, off) = unpack_uint16(msg, off)?;
        self.bits = bits;

        let (questions, off) = unpack_uint16(msg, off)?;
        self.questions = questions;

        let (answers, off) = unpack_uint16(msg, off)?;
        self.answers = answers;

        let (authorities, off) = unpack_uint16(msg, off)?;
        self.authorities = authorities;

        let (additionals, off) = unpack_uint16(msg, off)?;
        self.additionals = additionals;

        Ok(off)
    }

    pub(crate) fn header(&self) -> Header {
        Header {
            id: self.id,
            response: (self.bits & HEADER_BIT_QR) != 0,
            op_code: ((self.bits >> 11) & 0xF) as OpCode,
            authoritative: (self.bits & HEADER_BIT_AA) != 0,
            truncated: (self.bits & HEADER_BIT_TC) != 0,
            recursion_desired: (self.bits & HEADER_BIT_RD) != 0,
            recursion_available: (self.bits & HEADER_BIT_RA) != 0,
            rcode: RCode::from((self.bits & 0xF) as u8),
        }
    }
}
