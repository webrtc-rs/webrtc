#[cfg(test)]
mod ivf_writer_test;

use std::io::{Seek, SeekFrom, Write};

use byteorder::{LittleEndian, WriteBytesExt};
use bytes::{Bytes, BytesMut};
use rtp::packetizer::Depacketizer;

use crate::error::Result;
use crate::io::ivf_reader::IVFFileHeader;
use crate::io::Writer;

/// IVFWriter is used to take RTP packets and write them to an IVF on disk
pub struct IVFWriter<W: Write + Seek> {
    writer: W,
    count: u64,
    seen_key_frame: bool,
    current_frame: Option<BytesMut>,
    is_vp9: bool,
}

impl<W: Write + Seek> IVFWriter<W> {
    /// new initialize a new IVF writer with an io.Writer output
    pub fn new(writer: W, header: &IVFFileHeader) -> Result<Self> {
        let mut w = IVFWriter {
            writer,
            count: 0,
            seen_key_frame: false,
            current_frame: None,
            is_vp9: &header.four_cc != b"VP80",
        };

        w.write_header(header)?;

        Ok(w)
    }

    fn write_header(&mut self, header: &IVFFileHeader) -> Result<()> {
        self.writer.write_all(&header.signature)?; // DKIF
        self.writer.write_u16::<LittleEndian>(header.version)?; // version
        self.writer.write_u16::<LittleEndian>(header.header_size)?; // Header size
        self.writer.write_all(&header.four_cc)?; // FOURCC
        self.writer.write_u16::<LittleEndian>(header.width)?; // Width in pixels
        self.writer.write_u16::<LittleEndian>(header.height)?; // Height in pixels
        self.writer
            .write_u32::<LittleEndian>(header.timebase_denominator)?; // Framerate denominator
        self.writer
            .write_u32::<LittleEndian>(header.timebase_numerator)?; // Framerate numerator
        self.writer.write_u32::<LittleEndian>(header.num_frames)?; // Frame count, will be updated on first Close() call
        self.writer.write_u32::<LittleEndian>(header.unused)?; // Unused

        Ok(())
    }
}

impl<W: Write + Seek> Writer for IVFWriter<W> {
    /// write_rtp adds a new packet and writes the appropriate headers for it
    fn write_rtp(&mut self, packet: &rtp::packet::Packet) -> Result<()> {
        let mut depacketizer: Box<dyn Depacketizer> = if self.is_vp9 {
            Box::<rtp::codecs::vp9::Vp9Packet>::default()
        } else {
            Box::<rtp::codecs::vp8::Vp8Packet>::default()
        };

        let payload = depacketizer.depacketize(&packet.payload)?;

        let is_key_frame = payload[0] & 0x01;

        if (!self.seen_key_frame && is_key_frame == 1)
            || (self.current_frame.is_none() && !depacketizer.is_partition_head(&packet.payload))
        {
            return Ok(());
        }

        self.seen_key_frame = true;
        let frame_length = if let Some(current_frame) = &mut self.current_frame {
            current_frame.extend(payload);
            current_frame.len()
        } else {
            let mut current_frame = BytesMut::new();
            current_frame.extend(payload);
            let frame_length = current_frame.len();
            self.current_frame = Some(current_frame);
            frame_length
        };

        if !packet.header.marker {
            return Ok(());
        } else if let Some(current_frame) = &self.current_frame {
            if current_frame.is_empty() {
                return Ok(());
            }
        } else {
            return Ok(());
        }

        self.writer.write_u32::<LittleEndian>(frame_length as u32)?; // Frame length
        self.writer.write_u64::<LittleEndian>(self.count)?; // PTS
        self.count += 1;

        let frame_content = if let Some(current_frame) = self.current_frame.take() {
            current_frame.freeze()
        } else {
            Bytes::new()
        };

        self.writer.write_all(&frame_content)?;

        Ok(())
    }

    /// close stops the recording
    fn close(&mut self) -> Result<()> {
        // Update the frame count
        self.writer.seek(SeekFrom::Start(24))?;
        self.writer.write_u32::<LittleEndian>(self.count as u32)?;

        self.writer.flush()?;
        Ok(())
    }
}
