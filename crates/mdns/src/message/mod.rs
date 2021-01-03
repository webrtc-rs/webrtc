#[cfg(test)]
mod message_test;

pub mod builder;
pub mod header;
pub mod name;
mod packer;
pub mod parser;
pub mod question;
pub mod resource;

use header::*;
use packer::*;
use parser::*;
use question::*;
use resource::*;

use crate::errors::*;

use std::fmt;

use std::collections::HashMap;
use util::Error;

// Message formats

// A Type is a type of DNS request and response.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DNSType {
    // ResourceHeader.Type and question.Type
    A = 1,
    NS = 2,
    CNAME = 5,
    SOA = 6,
    PTR = 12,
    MX = 15,
    TXT = 16,
    AAAA = 28,
    SRV = 33,
    OPT = 41,

    // question.Type
    WKS = 11,
    HINFO = 13,
    MINFO = 14,
    AXFR = 252,
    ALL = 255,

    Unsupported = 0,
}

impl Default for DNSType {
    fn default() -> Self {
        DNSType::Unsupported
    }
}

impl From<u16> for DNSType {
    fn from(v: u16) -> Self {
        match v {
            1 => DNSType::A,
            2 => DNSType::NS,
            5 => DNSType::CNAME,
            6 => DNSType::SOA,
            12 => DNSType::PTR,
            15 => DNSType::MX,
            16 => DNSType::TXT,
            28 => DNSType::AAAA,
            33 => DNSType::SRV,
            41 => DNSType::OPT,

            // question.Type
            11 => DNSType::WKS,
            13 => DNSType::HINFO,
            14 => DNSType::MINFO,
            252 => DNSType::AXFR,
            255 => DNSType::ALL,

            _ => DNSType::Unsupported,
        }
    }
}

impl fmt::Display for DNSType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            DNSType::A => "A",
            DNSType::NS => "NS",
            DNSType::CNAME => "CNAME",
            DNSType::SOA => "SOA",
            DNSType::PTR => "PTR",
            DNSType::MX => "MX",
            DNSType::TXT => "TXT",
            DNSType::AAAA => "AAAA",
            DNSType::SRV => "SRV",
            DNSType::OPT => "OPT",
            DNSType::WKS => "WKS",
            DNSType::HINFO => "HINFO",
            DNSType::MINFO => "MINFO",
            DNSType::AXFR => "AXFR",
            DNSType::ALL => "ALL",
            _ => "Unsupported",
        };
        write!(f, "{}", s)
    }
}

impl DNSType {
    // pack_type appends the wire format of field to msg.
    pub(crate) fn pack(&self, msg: Vec<u8>) -> Vec<u8> {
        pack_uint16(msg, *self as u16)
    }

    pub(crate) fn unpack(&mut self, msg: &[u8], off: usize) -> Result<usize, Error> {
        let (t, o) = unpack_uint16(msg, off)?;
        *self = DNSType::from(t);
        Ok(o)
    }

    pub(crate) fn skip(msg: &[u8], off: usize) -> Result<usize, Error> {
        skip_uint16(msg, off)
    }
}

// A Class is a type of network.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DNSClass {
    // ResourceHeader.Class and question.Class
    INET = 1,
    CSNET = 2,
    CHAOS = 3,
    HESIOD = 4,

    // question.Class
    ANY = 255,
    Unsupported = 0,
}

impl Default for DNSClass {
    fn default() -> Self {
        DNSClass::Unsupported
    }
}

impl From<u16> for DNSClass {
    fn from(v: u16) -> Self {
        match v {
            1 => DNSClass::INET,
            2 => DNSClass::CSNET,
            3 => DNSClass::CHAOS,
            4 => DNSClass::HESIOD,

            // question.Class
            255 => DNSClass::ANY,

            _ => DNSClass::Unsupported,
        }
    }
}

impl fmt::Display for DNSClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            DNSClass::INET => "ClassINET",
            DNSClass::CSNET => "ClassCSNET",
            DNSClass::CHAOS => "ClassCHAOS",
            DNSClass::HESIOD => "ClassHESIOD",
            DNSClass::ANY => "ClassANY",
            DNSClass::Unsupported => "Unsupported",
        };
        write!(f, "{}", s)
    }
}

impl DNSClass {
    // pack_class appends the wire format of field to msg.
    pub(crate) fn pack(&self, msg: Vec<u8>) -> Vec<u8> {
        pack_uint16(msg, *self as u16)
    }

    pub(crate) fn unpack(&mut self, msg: &[u8], off: usize) -> Result<usize, Error> {
        let (c, o) = unpack_uint16(msg, off)?;
        *self = DNSClass::from(c);
        Ok(o)
    }

    pub(crate) fn skip(msg: &[u8], off: usize) -> Result<usize, Error> {
        skip_uint16(msg, off)
    }
}

// An OpCode is a DNS operation code.
pub type OpCode = u16;

// An RCode is a DNS response status code.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RCode {
    // Message.Rcode
    Success = 0,
    FormatError = 1,
    ServerFailure = 2,
    NameError = 3,
    NotImplemented = 4,
    Refused = 5,
    Unsupported,
}

impl Default for RCode {
    fn default() -> Self {
        RCode::Success
    }
}

impl From<u8> for RCode {
    fn from(v: u8) -> Self {
        match v {
            0 => RCode::Success,
            1 => RCode::FormatError,
            2 => RCode::ServerFailure,
            3 => RCode::NameError,
            4 => RCode::NotImplemented,
            5 => RCode::Refused,
            _ => RCode::Unsupported,
        }
    }
}

impl fmt::Display for RCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RCode::Success => "RCodeSuccess",
            RCode::FormatError => "RCodeFormatError",
            RCode::ServerFailure => "RCodeServerFailure",
            RCode::NameError => "RCodeNameError",
            RCode::NotImplemented => "RCodeNotImplemented",
            RCode::Refused => "RCodeRefused",
            RCode::Unsupported => "RCodeUnsupported",
        };
        write!(f, "{}", s)
    }
}

// Internal constants.

// PACK_STARTING_CAP is the default initial buffer size allocated during
// packing.
//
// The starting capacity doesn't matter too much, but most DNS responses
// Will be <= 512 bytes as it is the limit for DNS over UDP.
const PACK_STARTING_CAP: usize = 512;

// UINT16LEN is the length (in bytes) of a uint16.
const UINT16LEN: usize = 2;

// UINT32LEN is the length (in bytes) of a uint32.
const UINT32LEN: usize = 4;

// HEADER_LEN is the length (in bytes) of a DNS header.
//
// A header is comprised of 6 uint16s and no padding.
const HEADER_LEN: usize = 6 * UINT16LEN;

const HEADER_BIT_QR: u16 = 1 << 15; // query/response (response=1)
const HEADER_BIT_AA: u16 = 1 << 10; // authoritative
const HEADER_BIT_TC: u16 = 1 << 9; // truncated
const HEADER_BIT_RD: u16 = 1 << 8; // recursion desired
const HEADER_BIT_RA: u16 = 1 << 7; // recursion available

// Message is a representation of a DNS message.
#[derive(Default, Debug)]
pub struct Message {
    pub header: Header,
    pub questions: Vec<Question>,
    pub answers: Vec<Resource>,
    pub authorities: Vec<Resource>,
    pub additionals: Vec<Resource>,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = "dnsmessage.Message{Header: ".to_owned();
        s += self.header.to_string().as_str();

        s += ", Questions: ";
        let v: Vec<String> = self.questions.iter().map(|q| q.to_string()).collect();
        s += &v.join(", ");

        s += ", Answers: ";
        let v: Vec<String> = self.answers.iter().map(|q| q.to_string()).collect();
        s += &v.join(", ");

        s += ", Authorities: ";
        let v: Vec<String> = self.authorities.iter().map(|q| q.to_string()).collect();
        s += &v.join(", ");

        s += ", Additionals: ";
        let v: Vec<String> = self.additionals.iter().map(|q| q.to_string()).collect();
        s += &v.join(", ");

        write!(f, "{}", s)
    }
}

impl Message {
    // Unpack parses a full Message.
    pub fn unpack(&mut self, msg: &[u8]) -> Result<(), Error> {
        let mut p = Parser::default();
        self.header = p.start(msg)?;
        self.questions = p.all_questions()?;
        self.answers = p.all_answers()?;
        self.authorities = p.all_authorities()?;
        self.additionals = p.all_additionals()?;
        Ok(())
    }

    // Pack packs a full Message.
    pub fn pack(&mut self) -> Result<Vec<u8>, Error> {
        self.append_pack(vec![])
    }

    // append_pack is like Pack but appends the full Message to b and returns the
    // extended buffer.
    pub fn append_pack(&mut self, b: Vec<u8>) -> Result<Vec<u8>, Error> {
        // Validate the lengths. It is very unlikely that anyone will try to
        // pack more than 65535 of any particular type, but it is possible and
        // we should fail gracefully.
        if self.questions.len() > u16::MAX as usize {
            return Err(ERR_TOO_MANY_QUESTIONS.to_owned());
        }
        if self.answers.len() > u16::MAX as usize {
            return Err(ERR_TOO_MANY_ANSWERS.to_owned());
        }
        if self.authorities.len() > u16::MAX as usize {
            return Err(ERR_TOO_MANY_AUTHORITIES.to_owned());
        }
        if self.additionals.len() > u16::MAX as usize {
            return Err(ERR_TOO_MANY_ADDITIONALS.to_owned());
        }

        let (id, bits) = self.header.pack();

        let questions = self.questions.len() as u16;
        let answers = self.answers.len() as u16;
        let authorities = self.authorities.len() as u16;
        let additionals = self.additionals.len() as u16;

        let h = HeaderInternal {
            id,
            bits,
            questions,
            answers,
            authorities,
            additionals,
        };

        let compression_off = b.len();
        let mut msg = h.pack(b)?;

        // RFC 1035 allows (but does not require) compression for packing. RFC
        // 1035 requires unpacking implementations to support compression, so
        // unconditionally enabling it is fine.
        //
        // DNS lookups are typically done over UDP, and RFC 1035 states that UDP
        // DNS messages can be a maximum of 512 bytes long. Without compression,
        // many DNS response messages are over this limit, so enabling
        // compression will help ensure compliance.
        let mut compression = Some(HashMap::new());

        for question in &self.questions {
            msg = question.pack(msg, &mut compression, compression_off)?;
        }
        for answer in &mut self.answers {
            msg = answer.pack(msg, &mut compression, compression_off)?;
        }
        for authority in &mut self.authorities {
            msg = authority.pack(msg, &mut compression, compression_off)?;
        }
        for additional in &mut self.additionals {
            msg = additional.pack(msg, &mut compression, compression_off)?;
        }

        Ok(msg)
    }
}
