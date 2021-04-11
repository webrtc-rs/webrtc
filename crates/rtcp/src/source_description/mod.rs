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
pub enum SdesType {
    SdesEnd = 0,      // end of SDES list                RFC 3550, 6.5
    SdesCname = 1,    // canonical name                  RFC 3550, 6.5.1
    SdesName = 2,     // user name                       RFC 3550, 6.5.2
    SdesEmail = 3,    // user's electronic mail address  RFC 3550, 6.5.3
    SdesPhone = 4,    // user's phone number             RFC 3550, 6.5.4
    SdesLocation = 5, // geographic user location        RFC 3550, 6.5.5
    SdesTool = 6,     // name of application or tool     RFC 3550, 6.5.6
    SdesNote = 7,     // notice about the source         RFC 3550, 6.5.7
    SdesPrivate = 8,  // private extensions              RFC 3550, 6.5.8  (not implemented)
}

impl Default for SdesType {
    fn default() -> Self {
        SdesType::SdesEnd
    }
}

impl fmt::Display for SdesType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SdesType::SdesEnd => "END",
            SdesType::SdesCname => "CNAME",
            SdesType::SdesName => "NAME",
            SdesType::SdesEmail => "EMAIL",
            SdesType::SdesPhone => "PHONE",
            SdesType::SdesLocation => "LOC",
            SdesType::SdesTool => "TOOL",
            SdesType::SdesNote => "NOTE",
            SdesType::SdesPrivate => "PRIV",
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for SdesType {
    fn from(b: u8) -> Self {
        match b {
            1 => SdesType::SdesCname,
            2 => SdesType::SdesName,
            3 => SdesType::SdesEmail,
            4 => SdesType::SdesPhone,
            5 => SdesType::SdesLocation,
            6 => SdesType::SdesTool,
            7 => SdesType::SdesNote,
            8 => SdesType::SdesPrivate,
            _ => SdesType::SdesEnd,
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
