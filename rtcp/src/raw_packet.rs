use crate::{error::Error, header::*, packet::Packet, util::*};

use util::marshal::{Marshal, MarshalSize, Unmarshal};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::any::Any;
use std::fmt;

/// RawPacket represents an unparsed RTCP packet. It's returned by Unmarshal when
/// a packet with an unknown type is encountered.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct RawPacket(pub Bytes);

impl fmt::Display for RawPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawPacket: {self:?}")
    }
}

impl Packet for RawPacket {
    /// Header returns the Header associated with this packet.
    fn header(&self) -> Header {
        match Header::unmarshal(&mut self.0.clone()) {
            Ok(h) => h,
            Err(_) => Header::default(),
        }
    }

    /// destination_ssrc returns an array of SSRC values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn raw_size(&self) -> usize {
        self.0.len()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<RawPacket>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for RawPacket {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for RawPacket {
    /// Marshal encodes the packet in binary.
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, util::Error> {
        let h = Header::unmarshal(&mut self.0.clone())?;
        buf.put(self.0.clone());
        if h.padding {
            put_padding(buf, self.raw_size());
        }
        Ok(self.marshal_size())
    }
}

impl Unmarshal for RawPacket {
    /// Unmarshal decodes the packet from binary.
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self, util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        let raw_packet_len = raw_packet.remaining();
        if raw_packet_len < HEADER_LENGTH {
            return Err(Error::PacketTooShort.into());
        }

        let h = Header::unmarshal(raw_packet)?;

        let raw_hdr = h.marshal()?;
        let raw_body = raw_packet.copy_to_bytes(raw_packet.remaining());
        let mut raw = BytesMut::new();
        raw.extend(raw_hdr);
        raw.extend(raw_body);

        Ok(RawPacket(raw.freeze()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_raw_packet_roundtrip() -> Result<(), Error> {
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
            let result = pkt.marshal();
            assert_eq!(
                result.is_err(),
                unmarshal_error.is_some(),
                "Unmarshal {name}: err = {result:?}, want {unmarshal_error:?}"
            );

            if result.is_err() {
                continue;
            }

            let mut data = result.unwrap();

            let result = RawPacket::unmarshal(&mut data);

            assert_eq!(
                result.is_err(),
                unmarshal_error.is_some(),
                "Unmarshal {name}: err = {result:?}, want {unmarshal_error:?}"
            );

            if result.is_err() {
                continue;
            }

            let decoded = result.unwrap();

            assert_eq!(
                decoded, pkt,
                "{name} raw round trip: got {decoded:?}, want {pkt:?}"
            )
        }

        Ok(())
    }
}
