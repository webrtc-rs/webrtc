#[cfg(test)]
mod transport_cc_extension_test;

use bytes::{Buf, BufMut};
use serde::{Deserialize, Serialize};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;

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
#[derive(PartialEq, Eq, Debug, Default, Copy, Clone, Serialize, Deserialize)]
pub struct TransportCcExtension {
    pub transport_sequence: u16,
}

impl Unmarshal for TransportCcExtension {
    /// Unmarshal parses the passed byte slice and stores the result in the members
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self, util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        if raw_packet.remaining() < TRANSPORT_CC_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }
        let b0 = raw_packet.get_u8();
        let b1 = raw_packet.get_u8();

        let transport_sequence = ((b0 as u16) << 8) | b1 as u16;
        Ok(TransportCcExtension { transport_sequence })
    }
}

impl MarshalSize for TransportCcExtension {
    /// MarshalSize returns the size of the TransportCcExtension once marshaled.
    fn marshal_size(&self) -> usize {
        TRANSPORT_CC_EXTENSION_SIZE
    }
}

impl Marshal for TransportCcExtension {
    /// Marshal serializes the members to buffer
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, util::Error> {
        if buf.remaining_mut() < TRANSPORT_CC_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }
        buf.put_u16(self.transport_sequence);
        Ok(TRANSPORT_CC_EXTENSION_SIZE)
    }
}
