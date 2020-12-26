mod header_def;
mod header_test;

pub use header_def::Header;

/// PacketType specifies the type of an RTCP packet
/// RTCP packet types registered with IANA. See: https://www.iana.org/assignments/rtp-parameters/rtp-parameters.xhtml#rtp-parameters-4
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum PacketType {
    Unsupported = 0,
    SenderReport = 200,              // RFC 3550, 6.4.1
    ReceiverReport = 201,            // RFC 3550, 6.4.2
    SourceDescription = 202,         // RFC 3550, 6.5
    Goodbye = 203,                   // RFC 3550, 6.6
    ApplicationDefined = 204,        // RFC 3550, 6.7 (unimplemented)
    TransportSpecificFeedback = 205, // RFC 4585, 6051
    PayloadSpecificFeedback = 206,   // RFC 4585, 6.3
}

impl Default for PacketType {
    fn default() -> Self {
        PacketType::Unsupported
    }
}

/// Transport and Payload specific feedback messages overload the count field to act as a message type. those are listed here
pub const FORMAT_SLI: u8 = 2;
/// Transport and Payload specific feedback messages overload the count field to act as a message type. those are listed here
pub const FORMAT_PLI: u8 = 1;
/// Transport and Payload specific feedback messages overload the count field to act as a message type. those are listed here
pub const FORMAT_FIR: u8 = 4;
/// Transport and Payload specific feedback messages overload the count field to act as a message type. those are listed here
pub const FORMAT_TLN: u8 = 1;
/// Transport and Payload specific feedback messages overload the count field to act as a message type. those are listed here
pub const FORMAT_RRR: u8 = 5;
/// Transport and Payload specific feedback messages overload the count field to act as a message type. those are listed here
pub const FORMAT_REMB: u8 = 15;
/// Transport and Payload specific feedback messages overload the count field to act as a message type. those are listed here.
///
/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-5
pub const FORMAT_TCC: u8 = 15;

impl std::fmt::Display for PacketType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PacketType::Unsupported => "Unsupported",
            PacketType::SenderReport => "SR",
            PacketType::ReceiverReport => "RR",
            PacketType::SourceDescription => "SDES",
            PacketType::Goodbye => "BYE",
            PacketType::ApplicationDefined => "APP",
            PacketType::TransportSpecificFeedback => "TSFB",
            PacketType::PayloadSpecificFeedback => "PSFB",
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for PacketType {
    fn from(b: u8) -> Self {
        match b {
            200 => PacketType::SenderReport,              // RFC 3550, 6.4.1
            201 => PacketType::ReceiverReport,            // RFC 3550, 6.4.2
            202 => PacketType::SourceDescription,         // RFC 3550, 6.5
            203 => PacketType::Goodbye,                   // RFC 3550, 6.6
            204 => PacketType::ApplicationDefined,        // RFC 3550, 6.7 (unimplemented)
            205 => PacketType::TransportSpecificFeedback, // RFC 4585, 6051
            206 => PacketType::PayloadSpecificFeedback,   // RFC 4585, 6.3
            _ => PacketType::Unsupported,
        }
    }
}

pub const RTP_VERSION: u8 = 2;
pub const VERSION_SHIFT: u8 = 6;
pub const VERSION_MASK: u8 = 0x3;
pub const PADDING_SHIFT: u8 = 5;
pub const PADDING_MASK: u8 = 0x1;
pub const COUNT_SHIFT: u8 = 0;
pub const COUNT_MASK: u8 = 0x1f;

pub const HEADER_LENGTH: usize = 4;
pub const COUNT_MAX: usize = (1 << 5) - 1;
pub const SSRC_LENGTH: usize = 4;
pub const SDES_MAX_OCTET_COUNT: usize = (1 << 8) - 1;
