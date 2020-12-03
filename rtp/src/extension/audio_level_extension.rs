use std::io::{Read, Write};

use util::Error;

use byteorder::{ReadBytesExt, WriteBytesExt};

#[cfg(test)]
mod audio_level_extension_test;

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
    level: u8,
    voice: bool,
}

impl AudioLevelExtension {
    // Marshal serializes the members to buffer
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        if self.level > 127 {
            return Err(Error::new("audio level overflow".to_owned()));
        }
        let voice = if self.voice { 0x80u8 } else { 0u8 };

        writer.write_u8(voice | self.level)?;

        Ok(writer.flush()?)
    }

    // Unmarshal parses the passed byte slice and stores the result in the members
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let b = reader.read_u8()?;

        Ok(AudioLevelExtension {
            level: b & 0x7F,
            voice: (b & 0x80) != 0,
        })
    }
}
