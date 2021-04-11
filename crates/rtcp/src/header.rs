use crate::error::Error;

use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;

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

/// A Header is the common header shared by all RTCP packets
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Header {
    /// If the padding bit is set, this individual RTCP packet contains
    /// some additional padding octets at the end which are not part of
    /// the control information but are included in the length field.
    pub padding: bool,
    /// The number of reception reports, sources contained or FMT in this packet (depending on the Type)
    pub count: u8,
    /// The RTCP packet type for this packet
    pub packet_type: PacketType,
    /// The length of this RTCP packet in 32-bit words minus one,
    /// including the header and any padding.
    pub length: u16,
}

/// Marshal encodes the Header in binary
impl Header {
    pub fn marshal(&self) -> Result<BytesMut, Error> {
        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |V=2|P|    RC   |   PT=SR=200   |             length            |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        let mut raw_packet = BytesMut::new();
        raw_packet.resize(HEADER_LENGTH, 0u8);

        raw_packet[0] |= RTP_VERSION << VERSION_SHIFT;

        if self.padding {
            raw_packet[0] |= 1 << PADDING_SHIFT
        }

        if self.count > 31 {
            return Err(Error::InvalidHeader);
        }

        raw_packet[0] |= self.count << COUNT_SHIFT;

        raw_packet[1] = self.packet_type as u8;

        BigEndian::write_u16(&mut raw_packet[2..], self.length);

        Ok(raw_packet)
    }

    /// Unmarshal decodes the Header from binary
    pub fn unmarshal(&mut self, raw_packet: &mut BytesMut) -> Result<(), Error> {
        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |V=2|P|    RC   |      PT       |             length            |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        if raw_packet.len() < HEADER_LENGTH {
            return Err(Error::PacketTooShort);
        }

        let version = raw_packet[0] >> VERSION_SHIFT & VERSION_MASK;

        if version != RTP_VERSION {
            return Err(Error::BadVersion);
        }

        self.padding = (raw_packet[0] >> PADDING_SHIFT & PADDING_MASK) > 0;

        self.count = raw_packet[0] >> COUNT_SHIFT & COUNT_MASK;

        self.packet_type = PacketType::from(raw_packet[1]);

        self.length = BigEndian::read_u16(&raw_packet[2..]);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_header_unmarshal() {
        let tests = vec![
            (
                "valid",
                vec![
                    // v=2, p=0, count=1, RR, len=7
                    0x81u8, 0xc9, 0x00, 0x07,
                ],
                Header {
                    padding: false,
                    count: 1,
                    packet_type: PacketType::ReceiverReport,
                    length: 7,
                },
                Ok(()),
            ),
            (
                "also valid",
                vec![
                    // v=2, p=1, count=1, BYE, len=7
                    0xa1, 0xcc, 0x00, 0x07,
                ],
                Header {
                    padding: true,
                    count: 1,
                    packet_type: PacketType::ApplicationDefined,
                    length: 7,
                },
                Ok(()),
            ),
            (
                "bad version",
                vec![
                    // v=0, p=0, count=0, RR, len=4
                    0x00, 0xc9, 0x00, 0x04,
                ],
                Header {
                    padding: false,
                    count: 0,
                    packet_type: PacketType::Unsupported,
                    length: 0,
                },
                Err(Error::BadVersion),
            ),
        ];

        for (name, data, want, want_error) in tests {
            let mut h = Header::default();

            let got_error = h.unmarshal(&mut data.as_slice().into());

            assert_eq!(
                got_error, want_error,
                "Unmarshal {} header: err = {:?}, want {:?}",
                name, got_error, want_error
            );

            match got_error {
                Ok(_) => {
                    assert_eq!(
                        h, want,
                        "Unmarshal {} header: got {:?}, want {:?}",
                        name, h, want
                    )
                }

                Err(_) => continue,
            }
        }
    }

    #[test]
    fn test_header_roundtrip() {
        let tests = vec![
            (
                "valid",
                Header {
                    padding: true,
                    count: 31,
                    packet_type: PacketType::SenderReport,
                    length: 4,
                },
                Ok(()),
            ),
            (
                "also valid",
                Header {
                    padding: false,
                    count: 28,
                    packet_type: PacketType::ReceiverReport,
                    length: 65535,
                },
                Ok(()),
            ),
            (
                "invalid count",
                Header {
                    padding: false,
                    count: 40,
                    packet_type: PacketType::Unsupported,
                    length: 0,
                },
                Err(Error::InvalidHeader),
            ),
        ];

        for (name, header, want_error) in tests {
            let data = header.marshal();

            assert_eq!(
                data.clone().err(),
                want_error.clone().err(),
                "Marshal {}: err = {:?}, want {:?}",
                name,
                data,
                want_error
            );

            match data {
                Ok(mut e) => {
                    let mut decoded = Header::default();

                    decoded
                        .unmarshal(&mut e)
                        .expect(format!("Unmarshal {}", name).as_str());

                    assert_eq!(
                        decoded, header,
                        "{} header round trip: got {:?}, want {:?}",
                        name, decoded, header
                    )
                }

                Err(_) => continue,
            }
        }
    }
}
