#[cfg(test)]
mod playout_delay_extension_test;

use bytes::BufMut;
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;

pub const PLAYOUT_DELAY_EXTENSION_SIZE: usize = 3;
pub const PLAYOUT_DELAY_MAX_VALUE: u16 = (1 << 12) - 1;

/// PlayoutDelayExtension is an extension payload format described in
/// http://www.webrtc.org/experiments/rtp-hdrext/playout-delay
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |  ID   | len=2 |       MIN delay       |       MAX delay       |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(PartialEq, Eq, Debug, Default, Copy, Clone)]
pub struct PlayoutDelayExtension {
    pub min_delay: u16,
    pub max_delay: u16,
}

impl Unmarshal for PlayoutDelayExtension {
    /// Unmarshal parses the passed byte slice and stores the result in the members.
    fn unmarshal<B>(buf: &mut B) -> util::Result<Self>
    where
        Self: Sized,
        B: bytes::Buf,
    {
        if buf.remaining() < PLAYOUT_DELAY_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }

        let b0 = buf.get_u8();
        let b1 = buf.get_u8();
        let b2 = buf.get_u8();

        let min_delay = u16::from_be_bytes([b0, b1]) >> 4;
        let max_delay = u16::from_be_bytes([b1, b2]) & 0x0FFF;

        Ok(PlayoutDelayExtension {
            min_delay,
            max_delay,
        })
    }
}

impl MarshalSize for PlayoutDelayExtension {
    /// MarshalSize returns the size of the PlayoutDelayExtension once marshaled.
    fn marshal_size(&self) -> usize {
        PLAYOUT_DELAY_EXTENSION_SIZE
    }
}

impl Marshal for PlayoutDelayExtension {
    /// MarshalTo serializes the members to buffer
    fn marshal_to(&self, mut buf: &mut [u8]) -> util::Result<usize> {
        if buf.remaining_mut() < PLAYOUT_DELAY_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }
        if self.min_delay > PLAYOUT_DELAY_MAX_VALUE || self.max_delay > PLAYOUT_DELAY_MAX_VALUE {
            return Err(Error::PlayoutDelayOverflow.into());
        }

        buf.put_u8((self.min_delay >> 4) as u8);
        buf.put_u8(((self.min_delay << 4) as u8) | (self.max_delay >> 8) as u8);
        buf.put_u8(self.max_delay as u8);

        Ok(PLAYOUT_DELAY_EXTENSION_SIZE)
    }
}

impl PlayoutDelayExtension {
    pub fn new(min_delay: u16, max_delay: u16) -> Self {
        PlayoutDelayExtension {
            min_delay,
            max_delay,
        }
    }
}
