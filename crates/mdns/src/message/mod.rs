#[cfg(test)]
mod name_test;

pub mod body;
pub mod builder;
pub mod header;
pub mod name;
mod packer;
mod parser;
pub mod question;
pub mod resource;

use packer::*;

use std::fmt;

use util::Error;

// Message formats

// A Type is a type of DNS request and response.
#[derive(Copy, Clone)]
pub enum DNSType {
    // ResourceHeader.Type and Question.Type
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

    // Question.Type
    WKS = 11,
    HINFO = 13,
    MINFO = 14,
    AXFR = 252,
    ALL = 255,

    Unsupported,
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

            // Question.Type
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
            DNSType::A => "TypeA",
            DNSType::NS => "TypeNS",
            DNSType::CNAME => "TypeCNAME",
            DNSType::SOA => "TypeSOA",
            DNSType::PTR => "TypePTR",
            DNSType::MX => "TypeMX",
            DNSType::TXT => "TypeTXT",
            DNSType::AAAA => "TypeAAAA",
            DNSType::SRV => "TypeSRV",
            DNSType::OPT => "TypeOPT",
            DNSType::WKS => "TypeWKS",
            DNSType::HINFO => "TypeHINFO",
            DNSType::MINFO => "TypeMINFO",
            DNSType::AXFR => "TypeAXFR",
            DNSType::ALL => "TypeALL",
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
#[derive(Copy, Clone)]
pub enum DNSClass {
    // ResourceHeader.Class and Question.Class
    INET = 1,
    CSNET = 2,
    CHAOS = 3,
    HESIOD = 4,

    // Question.Class
    ANY = 255,
    Unsupported,
}

impl From<u16> for DNSClass {
    fn from(v: u16) -> Self {
        match v {
            1 => DNSClass::INET,
            2 => DNSClass::CSNET,
            3 => DNSClass::CHAOS,
            4 => DNSClass::HESIOD,

            // Question.Class
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
type OpCode = u16;

// An RCode is a DNS response status code.
#[derive(Copy, Clone)]
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

enum Section {
    NotStarted,
    Header,
    Questions,
    Answers,
    Authorities,
    Additionals,
    Done,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Section::NotStarted => "NotStarted",
            Section::Header => "Header",
            Section::Questions => "Question",
            Section::Answers => "Answer",
            Section::Authorities => "Authority",
            Section::Additionals => "Additional",
            Section::Done => "Done",
        };
        write!(f, "{}", s)
    }
}
