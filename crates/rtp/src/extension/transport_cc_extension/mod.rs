#[cfg(test)]
mod transport_cc_extension_test;

use crate::{error::Error, packetizer::Marshaller};

use anyhow::Result;
use bytes::{BufMut, Bytes, BytesMut};

// transport-wide sequence
pub const TRANSPORT_CC_EXTENSION_SIZE: usize = 2;

/// TransportCCExtension is a extension payload format in
/// https://tools.ietf.org/html/draft-holmer-rmcat-transport-wide-cc-extensions-01
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |       0xBE    |    0xDE       |           length=1            |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |  ID   | L=1   |transport-wide sequence number | zero padding  |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(PartialEq, Debug)]
pub struct TransportCcExtension {
    pub transport_sequence: u16,
}

impl Marshaller for TransportCcExtension {
    /// Unmarshal parses the passed byte slice and stores the result in the members
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        if raw_packet.len() < TRANSPORT_CC_EXTENSION_SIZE {
            return Err(Error::ErrTooSmall.into());
        }

        let transport_sequence = ((raw_packet[0] as u16) << 8) | raw_packet[1] as u16;
        Ok(TransportCcExtension { transport_sequence })
    }

    /// MarshalSize returns the size of the TransportCcExtension once marshaled.
    fn marshal_size(&self) -> usize {
        TRANSPORT_CC_EXTENSION_SIZE
    }

    /// Marshal serializes the members to buffer
    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        buf.put_u16(self.transport_sequence);
        Ok(TRANSPORT_CC_EXTENSION_SIZE)
    }
}
