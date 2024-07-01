#[cfg(test)]
mod audio_level_extension_test;

use bytes::{Buf, BufMut};
use serde::{Deserialize, Serialize};
use util::marshal::{Marshal, MarshalSize, Unmarshal};

use crate::error::Error;

// AUDIO_LEVEL_EXTENSION_SIZE One byte header size
pub const AUDIO_LEVEL_EXTENSION_SIZE: usize = 1;

/// AudioLevelExtension is a extension payload format described in
///
/// Implementation based on:
/// https://chromium.googlesource.com/external/webrtc/+/e2a017725570ead5946a4ca8235af27470ca0df9/webrtc/modules/rtp_rtcp/source/rtp_header_extensions.cc#49
///
/// One byte format:
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |  ID   | len=0 |V| level       |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// Two byte format:
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |      ID       |     len=1     |V|    level    |    0 (pad)    |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// ## Specifications
///
/// * [RFC 6464]
///
/// [RFC 6464]: https://tools.ietf.org/html/rfc6464
#[derive(PartialEq, Eq, Debug, Default, Copy, Clone, Serialize, Deserialize)]
pub struct AudioLevelExtension {
    pub level: u8,
    pub voice: bool,
}

impl Unmarshal for AudioLevelExtension {
    /// Unmarshal parses the passed byte slice and stores the result in the members
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self, util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        if raw_packet.remaining() < AUDIO_LEVEL_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }

        let b = raw_packet.get_u8();

        Ok(AudioLevelExtension {
            level: b & 0x7F,
            voice: (b & 0x80) != 0,
        })
    }
}

impl MarshalSize for AudioLevelExtension {
    /// MarshalSize returns the size of the AudioLevelExtension once marshaled.
    fn marshal_size(&self) -> usize {
        AUDIO_LEVEL_EXTENSION_SIZE
    }
}

impl Marshal for AudioLevelExtension {
    /// MarshalTo serializes the members to buffer
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, util::Error> {
        if buf.remaining_mut() < AUDIO_LEVEL_EXTENSION_SIZE {
            return Err(Error::ErrBufferTooSmall.into());
        }
        if self.level > 127 {
            return Err(Error::AudioLevelOverflow.into());
        }
        let voice = if self.voice { 0x80u8 } else { 0u8 };

        buf.put_u8(voice | self.level);

        Ok(AUDIO_LEVEL_EXTENSION_SIZE)
    }
}
