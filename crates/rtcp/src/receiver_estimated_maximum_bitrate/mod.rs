use std::fmt;

use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;

use crate::{
    errors::Error, header, header::Header, header::PacketType, packet::Packet, util::get_padding,
};

mod receiver_estimated_maximum_bitrate_test;

/// ReceiverEstimatedMaximumBitrate contains the receiver's estimated maximum bitrate.
/// see: https://tools.ietf.org/html/draft-alvestrand-rmcat-remb-03
#[derive(Debug, PartialEq, Default, Clone)]
pub struct ReceiverEstimatedMaximumBitrate {
    /// SSRC of sender
    pub sender_ssrc: u32,

    /// Estimated maximum bitrate
    pub bitrate: u64,

    /// SSRC entries which this packet applies to
    pub ssrcs: Vec<u32>,
}

const REMB_OFFSET: usize = 16;

// Keep a table of powers to units for fast conversion.

const BIT_UNITS: [&str; 7] = ["b", "Kb", "Mb", "Gb", "Tb", "Pb", "Eb"];
const UNIQUE_IDENTIFIER: [u8; 4] = [b'R', b'E', b'M', b'B'];

// String prints the REMB packet in a human-readable format.
impl fmt::Display for ReceiverEstimatedMaximumBitrate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Do some unit conversions because b/s is far too difficult to read.
        let mut bitrate = self.bitrate as f64;
        let mut powers = 0;

        // Keep dividing the bitrate until it's under 1000
        while bitrate >= 1000.0 && powers < BIT_UNITS.len() {
            bitrate /= 1000.0;
            powers += 1;
        }

        let unit = BIT_UNITS[powers];

        write!(
            f,
            "ReceiverEstimatedMaximumBitrate {:x} {:.2} {}/s",
            self.sender_ssrc, bitrate, unit,
        )
    }
}

impl Packet for ReceiverEstimatedMaximumBitrate {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Marshal serializes the packet and returns a byte slice.
    fn marshal(&self) -> Result<BytesMut, Error> {
        // Allocate a buffer of the exact output size.
        let mut buf = BytesMut::new();
        buf.resize(self.marshal_size(), 0u8);

        // Write to our buffer.
        let n = self.marshal_to(&mut buf)?;

        // This will always be true but just to be safe.
        if n != buf.len() {
            return Err(Error::WrongMarshalSize);
        }

        Ok(buf)
    }

    /// Unmarshal reads a REMB packet from the given byte slice.
    fn unmarshal(&mut self, buf: &mut BytesMut) -> Result<(), Error> {
        /*
            0                   1                   2                   3
            0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |V=2|P| FMT=15  |   PT=206      |             length            |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |                  SSRC of packet sender                        |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |                  SSRC of media source                         |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |  Unique identifier 'R' 'E' 'M' 'B'                            |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |  Num SSRC     | BR Exp    |  BR Mantissa                      |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |   SSRC feedback                                               |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |  ...                                                          |
        */

        // 20 bytes is the size of the packet with no SSRCs
        if buf.len() < 20 {
            return Err(Error::PacketTooShort);
        }

        // version  must be 2
        let version = buf[0] >> 6;
        if version != 2 {
            return Err(Error::Other(format!(
                "bad version: expected(2) actual({})",
                version
            )));
        }

        // padding must be unset
        let padding = (buf[0] >> 5) & 1;
        if padding != 0 {
            return Err(Error::WrongPadding);
        }

        // fmt must be 15
        let fmt_val = buf[0] & 31;
        if fmt_val != 15 {
            return Err(Error::WrongFeedbackType);
        }

        // Must be payload specific feedback
        if buf[1] != 206 {
            return Err(Error::WrongPayloadType);
        }

        // length is the number of 32-bit words, minus 1
        let length = BigEndian::read_u16(&buf[2..4]);
        let size = (length as usize + 1) * 4;

        // There's not way this could be legit
        if size < 20 {
            return Err(Error::HeaderTooSmall);
        }

        // Make sure the buffer is large enough.
        if buf.len() < size {
            return Err(Error::PacketTooShort);
        }

        // The sender SSRC is 32-bits
        self.sender_ssrc = BigEndian::read_u32(&buf[4..8]);

        // The destination SSRC must be 0
        let media = BigEndian::read_u32(&buf[8..12]);
        if media != 0 {
            return Err(Error::SSRCMustBeZero);
        }

        // REMB rules all around me
        if !buf[12..16].eq(&[b'R', b'E', b'M', b'B']) {
            return Err(Error::MissingREMBIdentifier);
        }

        // The next byte is the number of SSRC entries at the end.
        let num = buf[16] as usize;

        // Now we know the expected size, make sure they match.
        if size != 20 + 4 * num {
            return Err(Error::SSRCNumAndLengthMismatch);
        }

        // Get the 6-bit exponent value.
        let exp = buf[17] >> 2;

        // The remaining 2-bits plus the next 16-bits are the mantissa.
        let mantissa = ((buf[17] as u64) & 3) << 16 | (buf[18] as u64) << 8 | buf[19] as u64;

        if exp > 46 {
            // NOTE: We intentionally truncate values so they fit in a uint64.
            // Otherwise we would need a uint82.
            // This is 2.3 exabytes per second, which should be good enough.
            self.bitrate = !0
        } else {
            self.bitrate = mantissa << exp
        }

        // Clear any existing SSRCs
        self.ssrcs = vec![];

        let mut n = 20;

        // Loop over and parse the SSRC entires at the end.
        // We already verified that size == num * 4
        while n < size {
            let ssrc = BigEndian::read_u32(&buf[n..n + 4]);
            self.ssrcs.push(ssrc);

            n += 4;
        }

        Ok(())
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        self.ssrcs.clone()
    }

    fn trait_eq(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<ReceiverEstimatedMaximumBitrate>()
            .map_or(false, |a| self == a)
    }
}

impl ReceiverEstimatedMaximumBitrate {
    pub fn marshal_size(&self) -> usize {
        header::HEADER_LENGTH + REMB_OFFSET + self.ssrcs.len() * 4
    }

    /// MarshalTo serializes the packet to the given byte slice.
    pub fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize, Error> {
        /*
            0                   1                   2                   3
            0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |V=2|P| FMT=15  |   PT=206      |             length            |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |                  SSRC of packet sender                        |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |                  SSRC of media source                         |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |  Unique identifier 'R' 'E' 'M' 'B'                            |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |  Num SSRC     | BR Exp    |  BR Mantissa                      |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |   SSRC feedback                                               |
           +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
           |  ...                                                          |
        */

        let size = self.marshal_size();
        if buf.len() < size {
            return Err(Error::PacketTooShort);
        }

        buf[0] = 143; // v=2, p=0, fmt=15
        buf[1] = 206;

        // Length of this packet in 32-bit words minus one.
        let length = (self.marshal_size() / 4) - 1;
        BigEndian::write_u16(&mut buf[2..4], length as u16);

        BigEndian::write_u32(&mut buf[4..8], self.sender_ssrc);
        BigEndian::write_u32(&mut buf[8..12], 0); // always zero

        buf[12] = b'R';
        buf[13] = b'E';
        buf[14] = b'M';
        buf[15] = b'B';

        // Write the length of the ssrcs to follow at the end
        buf[16] = self.ssrcs.len() as u8;

        // We can only encode 18 bits of information in the mantissa.
        // The exponent lets us shift to the left up to 64 places (6-bits).
        // We actually need a uint82 to encode the largest possible number,
        // but uint64 should be good enough for 2.3 exabytes per second.

        // So we need to truncate the bitrate and use the exponent for the shift.
        // bitrate = mantissa * (1 << exp)

        // Calculate the total shift based on the leading number of zeroes.
        // This will be negative if there is no shift required.
        let shift = 64 - self.bitrate.leading_zeros();

        let mut _mantissa = 0usize;
        let mut exp = 0usize;

        if shift <= 18 {
            // Fit everything in the mantissa because we can.
            _mantissa = self.bitrate as usize;
        } else {
            // We can only use 18 bits of precision, so truncate.
            _mantissa = (self.bitrate >> (shift - 18)) as usize;
            exp = shift as usize - 18;
        }

        // We can't quite use the binary package because
        // a) it's a uint24 and b) the exponent is only 6-bits
        // Just trust me; this is big-endian encoding.
        buf[17] = ((exp << 2) | (_mantissa >> 16) as usize) as u8;
        buf[18] = (_mantissa >> 8) as u8;
        buf[19] = _mantissa as u8;

        // Write the SSRCs at the very end.
        let mut n = 20;
        for ssrc in self.ssrcs.clone() {
            BigEndian::write_u32(&mut buf[n..n + 4], ssrc);
            n += 4
        }

        Ok(n)
    }

    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        let l = self.marshal_size() + get_padding(self.marshal_size());

        Header {
            padding: get_padding(self.marshal_size()) != 0,
            count: header::FORMAT_REMB,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: ((l / 4) - 1) as u16,
        }
    }
}
