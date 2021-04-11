use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;

use crate::error::Error;

use super::PacketType;

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
    pub packet_type: super::PacketType,
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
        raw_packet.resize(super::HEADER_LENGTH, 0u8);

        raw_packet[0] |= super::RTP_VERSION << super::VERSION_SHIFT;

        if self.padding {
            raw_packet[0] |= 1 << super::PADDING_SHIFT
        }

        if self.count > 31 {
            return Err(Error::InvalidHeader);
        }

        raw_packet[0] |= self.count << super::COUNT_SHIFT;

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

        if raw_packet.len() < super::HEADER_LENGTH {
            return Err(Error::PacketTooShort);
        }

        let version = raw_packet[0] >> super::VERSION_SHIFT & super::VERSION_MASK;

        if version != super::RTP_VERSION {
            return Err(Error::BadVersion);
        }

        self.padding = (raw_packet[0] >> super::PADDING_SHIFT & super::PADDING_MASK) > 0;

        self.count = raw_packet[0] >> super::COUNT_SHIFT & super::COUNT_MASK;

        self.packet_type = PacketType::from(raw_packet[1]);

        self.length = BigEndian::read_u16(&raw_packet[2..]);

        Ok(())
    }
}
