#[cfg(test)]
mod receiver_estimated_maximum_bitrate_test;

use std::any::Any;
use std::fmt;

use bytes::{Buf, BufMut};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;
use crate::header::*;
use crate::packet::*;
use crate::util::*;

type Result<T> = std::result::Result<T, util::Error>;

/// ReceiverEstimatedMaximumBitrate contains the receiver's estimated maximum bitrate.
///
/// ## Specifications
///
/// * [draft-alvestrand-rmcat-remb-03]
///
/// [draft-alvestrand-rmcat-remb-03]: https://tools.ietf.org/html/draft-alvestrand-rmcat-remb-03
#[derive(Debug, PartialEq, Default, Clone)]
pub struct ReceiverEstimatedMaximumBitrate {
    /// SSRC of sender
    pub sender_ssrc: u32,

    /// Estimated maximum bitrate
    pub bitrate: f32,

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
        let mut bitrate = self.bitrate;
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
    /// Header returns the Header associated with this packet.
    fn header(&self) -> Header {
        Header {
            padding: get_padding_size(self.raw_size()) != 0,
            count: FORMAT_REMB,
            packet_type: PacketType::PayloadSpecificFeedback,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        self.ssrcs.clone()
    }

    fn raw_size(&self) -> usize {
        HEADER_LENGTH + REMB_OFFSET + self.ssrcs.len() * 4
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<ReceiverEstimatedMaximumBitrate>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for ReceiverEstimatedMaximumBitrate {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for ReceiverEstimatedMaximumBitrate {
    /// Marshal serializes the packet and returns a byte slice.
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        const BITRATE_MAX: f32 = 2.417_842_4e24; //0x3FFFFp+63;

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

        if buf.remaining_mut() < self.marshal_size() {
            return Err(Error::BufferTooShort.into());
        }

        let h = self.header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put_u32(self.sender_ssrc);
        buf.put_u32(0); // always zero

        buf.put_slice(&UNIQUE_IDENTIFIER);

        // Write the length of the ssrcs to follow at the end
        buf.put_u8(self.ssrcs.len() as u8);

        let mut exp = 0;
        let mut bitrate = self.bitrate;
        if bitrate >= BITRATE_MAX {
            bitrate = BITRATE_MAX
        }

        if bitrate < 0.0 {
            return Err(Error::InvalidBitrate.into());
        }

        while bitrate >= (1 << 18) as f32 {
            bitrate /= 2.0;
            exp += 1;
        }

        if exp >= (1 << 6) {
            return Err(Error::InvalidBitrate.into());
        }

        let mantissa = bitrate.floor() as u32;

        // We can't quite use the binary package because
        // a) it's a uint24 and b) the exponent is only 6-bits
        // Just trust me; this is big-endian encoding.
        buf.put_u8((exp << 2) as u8 | (mantissa >> 16) as u8);
        buf.put_u8((mantissa >> 8) as u8);
        buf.put_u8(mantissa as u8);

        // Write the SSRCs at the very end.
        for ssrc in &self.ssrcs {
            buf.put_u32(*ssrc);
        }

        if h.padding {
            put_padding(buf, self.raw_size());
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for ReceiverEstimatedMaximumBitrate {
    /// Unmarshal reads a REMB packet from the given byte slice.
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        let raw_packet_len = raw_packet.remaining();
        // 20 bytes is the size of the packet with no SSRCs
        if raw_packet_len < 20 {
            return Err(Error::PacketTooShort.into());
        }

        const MANTISSA_MAX: u32 = 0x7FFFFF;
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

        let sender_ssrc = raw_packet.get_u32();
        let media_ssrc = raw_packet.get_u32();
        if media_ssrc != 0 {
            return Err(Error::SsrcMustBeZero.into());
        }

        // REMB rules all around me
        let mut unique_identifier = [0; 4];
        unique_identifier[0] = raw_packet.get_u8();
        unique_identifier[1] = raw_packet.get_u8();
        unique_identifier[2] = raw_packet.get_u8();
        unique_identifier[3] = raw_packet.get_u8();
        if unique_identifier[0] != UNIQUE_IDENTIFIER[0]
            || unique_identifier[1] != UNIQUE_IDENTIFIER[1]
            || unique_identifier[2] != UNIQUE_IDENTIFIER[2]
            || unique_identifier[3] != UNIQUE_IDENTIFIER[3]
        {
            return Err(Error::MissingRembIdentifier.into());
        }

        // The next byte is the number of SSRC entries at the end.
        let ssrcs_len = raw_packet.get_u8() as usize;

        // Get the 6-bit exponent value.
        let b17 = raw_packet.get_u8();
        let mut exp = (b17 as u64) >> 2;
        exp += 127; // bias for IEEE754
        exp += 23; // IEEE754 biases the decimal to the left, abs-send-time biases it to the right

        // The remaining 2-bits plus the next 16-bits are the mantissa.
        let b18 = raw_packet.get_u8();
        let b19 = raw_packet.get_u8();
        let mut mantissa = ((b17 & 3) as u32) << 16 | (b18 as u32) << 8 | b19 as u32;

        if mantissa != 0 {
            // ieee754 requires an implicit leading bit
            while (mantissa & (MANTISSA_MAX + 1)) == 0 {
                exp -= 1;
                mantissa *= 2;
            }
        }

        // bitrate = mantissa * 2^exp
        let bitrate = f32::from_bits(((exp as u32) << 23) | (mantissa & MANTISSA_MAX));

        let mut ssrcs = vec![];
        for _i in 0..ssrcs_len {
            ssrcs.push(raw_packet.get_u32());
        }

        if
        /*header.padding &&*/
        raw_packet.has_remaining() {
            raw_packet.advance(raw_packet.remaining());
        }

        Ok(ReceiverEstimatedMaximumBitrate {
            sender_ssrc,
            //media_ssrc,
            bitrate,
            ssrcs,
        })
    }
}
