use crate::{error::Error, header::*, packet::Packet};

use anyhow::Result;
use bytes::Bytes;
use std::any::Any;
use std::fmt;

/// RawPacket represents an unparsed RTCP packet. It's returned by Unmarshal when
/// a packet with an unknown type is encountered.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct RawPacket(pub Bytes);

impl fmt::Display for RawPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawPacket: {:?}", self)
    }
}

impl Packet for RawPacket {
    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn size(&self) -> usize {
        self.0.len()
    }

    /// Marshal encodes the packet in binary.
    fn marshal(&self) -> Result<Bytes> {
        Ok(self.0.clone())
    }

    /// Unmarshal decodes the packet from binary.
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        if raw_packet.len() < HEADER_LENGTH {
            return Err(Error::PacketTooShort.into());
        }

        let _ = Header::unmarshal(raw_packet)?;

        Ok(RawPacket(raw_packet.clone()))
    }

    fn equal_to(&self, other: &dyn Packet) -> bool {
        other
            .as_any()
            .downcast_ref::<RawPacket>()
            .map_or(false, |a| self == a)
    }

    fn clone_to(&self) -> Box<dyn Packet> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl RawPacket {
    /// Header returns the Header associated with this packet.
    pub fn header(&self) -> Header {
        match Header::unmarshal(&self.0) {
            Ok(h) => h,
            Err(_) => Header::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_raw_packet_roundtrip() -> Result<()> {
        let tests: Vec<(&str, RawPacket, Option<Error>)> = vec![
            (
                "valid",
                RawPacket(Bytes::from_static(&[
                    0x81, 0xcb, 0x00, 0x0c, // v=2, p=0, count=1, BYE, len=12
                    0x90, 0x2f, 0x9e, 0x2e, // ssrc=0x902f9e2e
                    0x03, 0x46, 0x4f, 0x4f, // len=3, text=FOO
                ])),
                None,
            ),
            (
                "short header",
                RawPacket(Bytes::from_static(&[0x80])),
                Some(Error::PacketTooShort),
            ),
            (
                "invalid header",
                RawPacket(
                    // v=0, p=0, count=0, RR, len=4
                    Bytes::from_static(&[0x00, 0xc9, 0x00, 0x04]),
                ),
                Some(Error::BadVersion),
            ),
        ];

        for (name, pkt, unmarshal_error) in tests {
            let data = pkt.marshal()?;
            let result = RawPacket::unmarshal(&data);

            assert_eq!(
                result.is_err(),
                unmarshal_error.is_some(),
                "Unmarshal {}: err = {:?}, want {:?}",
                name,
                result,
                unmarshal_error
            );

            if result.is_err() {
                continue;
            }

            let decoded = result.unwrap();

            assert_eq!(
                decoded, pkt,
                "{} raw round trip: got {:?}, want {:?}",
                name, decoded, pkt
            )
        }

        Ok(())
    }
}
