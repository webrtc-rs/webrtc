use std::fmt;
use std::io::{Read, Write};

use util::Error;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::errors::*;

#[cfg(test)]
mod header_test;

// PacketType specifies the type of an RTCP packet
// RTCP packet types registered with IANA. See: https://www.iana.org/assignments/rtp-parameters/rtp-parameters.xhtml#rtp-parameters-4

#[derive(Debug, Copy, Clone, PartialEq)]
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

// Transport and Payload specific feedback messages overload the count field to act as a message type. those are listed here
pub const FORMAT_SLI: u8 = 2;
pub const FORMAT_PLI: u8 = 1;
pub const FORMAT_FIR: u8 = 4;
pub const FORMAT_TLN: u8 = 1;
pub const FORMAT_RRR: u8 = 5;
pub const FORMAT_REMB: u8 = 15;
//https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01#page-5
pub const FORMAT_TCC: u8 = 15;

impl fmt::Display for PacketType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

const RTP_VERSION: u8 = 2;

// A Header is the common header shared by all RTCP packets
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Header {
    // If the padding bit is set, this individual RTCP packet contains
    // some additional padding octets at the end which are not part of
    // the control information but are included in the length field.
    pub padding: bool,
    // The number of reception reports, sources contained or FMT in this packet (depending on the Type)
    pub count: u8,
    // The RTCP packet type for this packet
    pub packet_type: PacketType,
    // The length of this RTCP packet in 32-bit words minus one,
    // including the header and any padding.
    pub length: u16,
}

const VERSION_SHIFT: u8 = 6;
const VERSION_MASK: u8 = 0x3;
const PADDING_SHIFT: u8 = 5;
const PADDING_MASK: u8 = 0x1;
const COUNT_SHIFT: u8 = 0;
const COUNT_MASK: u8 = 0x1f;

pub const HEADER_LENGTH: usize = 4;
pub const COUNT_MAX: usize = (1 << 5) - 1;
pub const SSRC_LENGTH: usize = 4;
pub const SDES_MAX_OCTET_COUNT: usize = (1 << 8) - 1;

// Marshal encodes the Header in binary
impl Header {
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |V=2|P|    RC   |   PT=SR=200   |             length            |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        let mut b0 = RTP_VERSION << VERSION_SHIFT;

        if self.padding {
            b0 |= 1 << PADDING_SHIFT
        }

        if self.count > 31 {
            return Err(ERR_INVALID_HEADER.clone());
        }
        b0 |= self.count << COUNT_SHIFT;
        writer.write_u8(b0)?;

        let b1 = self.packet_type as u8;
        writer.write_u8(b1)?;

        writer.write_u16::<BigEndian>(self.length)?;

        Ok(writer.flush()?)
    }

    // Unmarshal decodes the Header from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |V=2|P|    RC   |      PT       |             length            |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */
        let b0 = reader.read_u8()?;
        let version = b0 >> VERSION_SHIFT & VERSION_MASK;
        if version != RTP_VERSION {
            return Err(ERR_BAD_VERSION.clone());
        }

        let padding = (b0 >> PADDING_SHIFT & PADDING_MASK) > 0;
        let count = b0 >> COUNT_SHIFT & COUNT_MASK;

        let b1 = reader.read_u8()?;
        let packet_type: PacketType = b1.into();
        if packet_type == PacketType::Unsupported {
            return Err(ERR_WRONG_TYPE.clone());
        }

        let length = reader.read_u16::<BigEndian>()?;

        Ok(Header {
            padding,
            // The number of reception reports, sources contained or FMT in this packet (depending on the Type)
            count,
            // The RTCP packet type for this packet
            packet_type,
            // The length of this RTCP packet in 32-bit words minus one,
            // including the header and any padding.
            length,
        })
    }
}
