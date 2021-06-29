#[cfg(test)]
mod audio_level_extension_test;

use crate::{error::Error, packetizer::Marshaller};

use anyhow::Result;
use bytes::{BufMut, Bytes, BytesMut};

// AUDIO_LEVEL_EXTENSION_SIZE One byte header size
pub const AUDIO_LEVEL_EXTENSION_SIZE: usize = 1;

/// AudioLevelExtension is a extension payload format described in
/// https://tools.ietf.org/html/rfc6464
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
#[derive(PartialEq, Debug)]
pub struct AudioLevelExtension {
    pub level: u8,
    pub voice: bool,
}

impl Marshaller for AudioLevelExtension {
    /// Unmarshal parses the passed byte slice and stores the result in the members
    fn unmarshal(raw_packet: &Bytes) -> Result<Self> {
        if raw_packet.len() < AUDIO_LEVEL_EXTENSION_SIZE {
            return Err(Error::ErrTooSmall.into());
        }

        let b = raw_packet[0];

        Ok(AudioLevelExtension {
            level: b & 0x7F,
            voice: (b & 0x80) != 0,
        })
    }

    /// MarshalSize returns the size of the AudioLevelExtension once marshaled.
    fn marshal_size(&self) -> usize {
        AUDIO_LEVEL_EXTENSION_SIZE
    }

    /// MarshalTo serializes the members to buffer
    fn marshal_to(&self, buf: &mut BytesMut) -> Result<usize> {
        if self.level > 127 {
            return Err(Error::AudioLevelOverflow.into());
        }
        let voice = if self.voice { 0x80u8 } else { 0u8 };

        buf.put_u8(voice | self.level);

        Ok(AUDIO_LEVEL_EXTENSION_SIZE)
    }
}
