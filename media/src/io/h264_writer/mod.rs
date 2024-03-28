#[cfg(test)]
mod h264_writer_test;

use std::io::{Seek, Write};

use rtp::codecs::h264::H264Packet;
use rtp::packetizer::Depacketizer;

use crate::error::Result;
use crate::io::Writer;

const NALU_TTYPE_STAP_A: u32 = 24;
const NALU_TTYPE_SPS: u32 = 7;
const NALU_TYPE_BITMASK: u32 = 0x1F;

fn is_key_frame(data: &[u8]) -> bool {
    if data.len() < 4 {
        false
    } else {
        let word = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let nalu_type = (word >> 24) & NALU_TYPE_BITMASK;
        (nalu_type == NALU_TTYPE_STAP_A && (word & NALU_TYPE_BITMASK) == NALU_TTYPE_SPS)
            || (nalu_type == NALU_TTYPE_SPS)
    }
}

/// H264Writer is used to take RTP packets, parse them and
/// write the data to an io.Writer.
/// Currently it only supports non-interleaved mode
/// Therefore, only 1-23, 24 (STAP-A), 28 (FU-A) NAL types are allowed.
/// <https://tools.ietf.org/html/rfc6184#section-5.2>
pub struct H264Writer<W: Write + Seek> {
    writer: W,
    has_key_frame: bool,
    cached_packet: Option<H264Packet>,
}

impl<W: Write + Seek> H264Writer<W> {
    // new initializes a new H264 writer with an io.Writer output
    pub fn new(writer: W) -> Self {
        H264Writer {
            writer,
            has_key_frame: false,
            cached_packet: None,
        }
    }
}

impl<W: Write + Seek> Writer for H264Writer<W> {
    /// write_rtp adds a new packet and writes the appropriate headers for it
    fn write_rtp(&mut self, packet: &rtp::packet::Packet) -> Result<()> {
        if packet.payload.is_empty() {
            return Ok(());
        }

        if !self.has_key_frame {
            self.has_key_frame = is_key_frame(&packet.payload);
            if !self.has_key_frame {
                // key frame not defined yet. discarding packet
                return Ok(());
            }
        }

        if self.cached_packet.is_none() {
            self.cached_packet = Some(H264Packet::default());
        }

        if let Some(cached_packet) = &mut self.cached_packet {
            let payload = cached_packet.depacketize(&packet.payload)?;

            self.writer.write_all(&payload)?;
        }

        Ok(())
    }

    /// close closes the underlying writer
    fn close(&mut self) -> Result<()> {
        self.cached_packet = None;
        self.writer.flush()?;
        Ok(())
    }
}
