use std::io::{Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

use crate::error::Error;

#[cfg(test)]
mod audio_level_extension_test;

const AUDIO_LEVEL_EXTENSION_SIZE: usize = 1;

// AudioLevelExtension is a extension payload format described in
// https://tools.ietf.org/html/rfc6464
//
// Implementation based on:
// https://chromium.googlesource.com/external/webrtc/+/e2a017725570ead5946a4ca8235af27470ca0df9/webrtc/modules/rtp_rtcp/source/rtp_header_extensions.cc#49
//
// One byte format:
// 0                   1
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |  ID   | len=0 |V| level       |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
// Two byte format:
// 0                   1                   2                   3
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |      ID       |     len=1     |V|    level    |    0 (pad)    |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(PartialEq, Debug)]
pub struct AudioLevelExtension {
    pub level: u8,
    pub voice: bool,
}

impl AudioLevelExtension {
    // Marshal serializes the members to buffer
    pub fn marshal(&self) -> Result<BytesMut, ExtensionError> {
        if self.level > 127 {
            return Err(Error::AudioLevelOverflow);
        }

        let voice = if self.voice { 0x80u8 } else { 0u8 };

        let mut buf = vec![0u8; AUDIO_LEVEL_EXTENSION_SIZE];
        buf[0] = voice | self.level;

        Ok(buf.as_slice().into())
    }

    // Unmarshal parses the passed byte slice and stores the result in the members
    pub fn unmarshal(&mut self, raw_data: &mut BytesMut) -> Result<(), ExtensionError> {
        if raw_data.len() < AUDIO_LEVEL_EXTENSION_SIZE {
            return Err(ExtensionError::TooSmall);
        }

        self.level = raw_data[0] & 0x7F;
        self.voice = (raw_data[0] & 0x80) != 0;
        Ok(())
    }
}
