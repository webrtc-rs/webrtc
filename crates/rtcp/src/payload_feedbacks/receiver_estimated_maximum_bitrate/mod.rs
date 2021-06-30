#[cfg(test)]
mod receiver_estimated_maximum_bitrate_test;

use crate::{error::Error, header::*, packet::*, util::*};

use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::any::Any;
use std::fmt;

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

/// Keep a table of powers to units for fast conversion.
const BIT_UNITS: [&str; 7] = ["b", "Kb", "Mb", "Gb", "Tb", "Pb", "Eb"];
const UNIQUE_IDENTIFIER: [u8; 4] = [b'R', b'E', b'M', b'B'];

/// String prints the REMB packet in a human-readable format.
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
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        self.ssrcs.clone()
    }

    fn size(&self) -> usize {
        HEADER_LENGTH + REMB_OFFSET + self.ssrcs.len() * 4
    }

    /// Marshal serializes the packet and returns a byte slice.
    fn marshal(&self) -> Result<Bytes> {
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

        // Allocate a buffer of the exact output size.
        let mut writer = BytesMut::with_capacity(self.marshal_size());

        let h = self.header();
        let data = h.marshal()?;
        writer.extend(data);

        writer.put_u32(self.sender_ssrc);
        writer.put_u32(0); // always zero

        writer.put_slice(&UNIQUE_IDENTIFIER);

        // Write the length of the ssrcs to follow at the end
        writer.put_u8(self.ssrcs.len() as u8);

        // We can only encode 18 bits of information in the mantissa.
        // The exponent lets us shift to the left up to 64 places (6-bits).
        // We actually need a uint82 to encode the largest possible number,
        // but uint64 should be good enough for 2.3 exabytes per second.

        // So we need to truncate the bitrate and use the exponent for the shift.
        // bitrate = mantissa * (1 << exp)

        // Calculate the total shift based on the leading number of zeroes.
        // This will be negative if there is no shift required.
        let shift = 64 - self.bitrate.leading_zeros();
        let mantissa;
        let exp;

        if shift <= 18 {
            // Fit everything in the mantissa because we can.
            mantissa = self.bitrate;
            exp = 0;
        } else {
            // We can only use 18 bits of precision, so truncate.
            mantissa = self.bitrate >> (shift - 18);
            exp = shift - 18;
        }

        // We can't quite use the binary package because
        // a) it's a uint24 and b) the exponent is only 6-bits
        // Just trust me; this is big-endian encoding.
        writer.put_u8(((exp << 2) | (mantissa >> 16) as u32) as u8);
        writer.put_u8((mantissa >> 8) as u8);
        writer.put_u8(mantissa as u8);

        // Write the SSRCs at the very end.
        for ssrc in &self.ssrcs {
            writer.put_u32(*ssrc);
        }

        put_padding(&mut writer);
        Ok(writer.freeze())
    }

    /// Unmarshal reads a REMB packet from the given byte slice.
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        // 20 bytes is the size of the packet with no SSRCs
        if raw_packet.len() < 20 {
            return Err(Error::PacketTooShort.into());
        }
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
        let header = Header::unmarshal(raw_packet)?;

        if header.packet_type != PacketType::PayloadSpecificFeedback || header.count != FORMAT_REMB
        {
            return Err(Error::WrongType.into());
        }

        let reader = &mut raw_packet.slice(HEADER_LENGTH..);

        let sender_ssrc = reader.get_u32();
        let media_ssrc = reader.get_u32();
        if media_ssrc != 0 {
            return Err(Error::SsrcMustBeZero.into());
        }

        // REMB rules all around me
        let mut unique_identifier = vec![0; 4];
        unique_identifier[0] = reader.get_u8();
        unique_identifier[1] = reader.get_u8();
        unique_identifier[2] = reader.get_u8();
        unique_identifier[3] = reader.get_u8();
        if unique_identifier[0] != UNIQUE_IDENTIFIER[0]
            || unique_identifier[1] != UNIQUE_IDENTIFIER[1]
            || unique_identifier[2] != UNIQUE_IDENTIFIER[2]
            || unique_identifier[3] != UNIQUE_IDENTIFIER[3]
        {
            return Err(Error::MissingRembIdentifier.into());
        }

        // The next byte is the number of SSRC entries at the end.
        let ssrcs_len = reader.get_u8() as usize;

        // Get the 6-bit exponent value.
        let b17 = reader.get_u8();
        let exp = (b17 as u64) >> 2;

        // The remaining 2-bits plus the next 16-bits are the mantissa.
        let b18 = reader.get_u8();
        let b19 = reader.get_u8();
        let mantissa = ((b17 & 3) as u64) << 16 | (b18 as u64) << 8 | b19 as u64;

        let bitrate = if exp > 46 {
            // NOTE: We intentionally truncate values so they fit in a uint64.
            // Otherwise we would need a uint82.
            // This is 2.3 exabytes per second, which should be good enough.
            std::u64::MAX
        } else {
            mantissa << exp
        };

        let mut ssrcs = vec![];
        for _i in 0..ssrcs_len {
            ssrcs.push(reader.get_u32());
        }

        Ok(ReceiverEstimatedMaximumBitrate {
            sender_ssrc,
            //media_ssrc,
            bitrate,
            ssrcs,
        })
    }

    fn equal_to(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<ReceiverEstimatedMaximumBitrate>()
            .map_or(false, |a| self == a)
    }

    fn clone_to(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ReceiverEstimatedMaximumBitrate {
    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        Header {
            padding: get_padding(self.size()) != 0,
            count: FORMAT_REMB,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }
}
