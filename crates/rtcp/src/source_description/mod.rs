use std::fmt;

mod source_description_def;
mod source_description_test;

pub use source_description_def::{
    SourceDescription, SourceDescriptionChunk, SourceDescriptionItem,
};

/// SDESType is the item type used in the RTCP SDES control packet.
/// RTP SDES item types registered with IANA. See: https://www.iana.org/assignments/rtp-parameters/rtp-parameters.xhtml#rtp-parameters-5
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum SDESType {
    SDESEnd = 0,      // end of SDES list                RFC 3550, 6.5
    SDESCNAME = 1,    // canonical name                  RFC 3550, 6.5.1
    SDESName = 2,     // user name                       RFC 3550, 6.5.2
    SDESEmail = 3,    // user's electronic mail address  RFC 3550, 6.5.3
    SDESPhone = 4,    // user's phone number             RFC 3550, 6.5.4
    SDESLocation = 5, // geographic user location        RFC 3550, 6.5.5
    SDESTool = 6,     // name of application or tool     RFC 3550, 6.5.6
    SDESNote = 7,     // notice about the source         RFC 3550, 6.5.7
    SDESPrivate = 8,  // private extensions              RFC 3550, 6.5.8  (not implemented)
}

impl Default for SDESType {
    fn default() -> Self {
        SDESType::SDESEnd
    }
}

impl fmt::Display for SDESType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SDESType::SDESEnd => "END",
            SDESType::SDESCNAME => "CNAME",
            SDESType::SDESName => "NAME",
            SDESType::SDESEmail => "EMAIL",
            SDESType::SDESPhone => "PHONE",
            SDESType::SDESLocation => "LOC",
            SDESType::SDESTool => "TOOL",
            SDESType::SDESNote => "NOTE",
            SDESType::SDESPrivate => "PRIV",
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for SDESType {
    fn from(b: u8) -> Self {
        match b {
            1 => SDESType::SDESCNAME,
            2 => SDESType::SDESName,
            3 => SDESType::SDESEmail,
            4 => SDESType::SDESPhone,
            5 => SDESType::SDESLocation,
            6 => SDESType::SDESTool,
            7 => SDESType::SDESNote,
            8 => SDESType::SDESPrivate,
            _ => SDESType::SDESEnd,
        }
    }
}

const SDES_SOURCE_LEN: usize = 4;
const SDES_TYPE_LEN: usize = 1;
const SDES_TYPE_OFFSET: usize = 0;
const SDES_OCTET_COUNT_LEN: usize = 1;
const SDES_OCTET_COUNT_OFFSET: usize = 1;
const SDES_MAX_OCTET_COUNT: usize = (1 << 8) - 1;
const SDES_TEXT_OFFSET: usize = 2;
