#[cfg(test)]
mod ogg_writer_test;

use std::io::{BufWriter, Seek, Write};

use byteorder::{LittleEndian, WriteBytesExt};
use bytes::Bytes;
use rtp::packetizer::Depacketizer;

use crate::error::Result;
use crate::io::ogg_reader::*;
use crate::io::Writer;

/// OggWriter is used to take RTP packets and write them to an OGG on disk
pub struct OggWriter<W: Write + Seek> {
    writer: W,
    sample_rate: u32,
    channel_count: u8,
    serial: u32,
    page_index: u32,
    checksum_table: [u32; 256],
    previous_granule_position: u64,
    previous_timestamp: u32,
    last_payload_size: usize,
    last_payload: Bytes,
}

impl<W: Write + Seek> OggWriter<W> {
    /// new initialize a new OGG Opus writer with an io.Writer output
    pub fn new(writer: W, sample_rate: u32, channel_count: u8) -> Result<Self> {
        let mut w = OggWriter {
            writer,
            sample_rate,
            channel_count,
            serial: rand::random::<u32>(),
            page_index: 0,
            checksum_table: generate_checksum_table(),

            // Timestamp and Granule MUST start from 1
            // Only headers can have 0 values
            previous_timestamp: 1,
            previous_granule_position: 1,
            last_payload_size: 0,
            last_payload: Bytes::new(),
        };

        w.write_headers()?;

        Ok(w)
    }

    /*
        ref: https://tools.ietf.org/html/rfc7845.html
        https://git.xiph.org/?p=opus-tools.git;a=blob;f=src/opus_header.c#l219

           Page 0         Pages 1 ... n        Pages (n+1) ...
        +------------+ +---+ +---+ ... +---+ +-----------+ +---------+ +--
        |            | |   | |   |     |   | |           | |         | |
        |+----------+| |+-----------------+| |+-------------------+ +-----
        |||ID Header|| ||  Comment Header || ||Audio Data Packet 1| | ...
        |+----------+| |+-----------------+| |+-------------------+ +-----
        |            | |   | |   |     |   | |           | |         | |
        +------------+ +---+ +---+ ... +---+ +-----------+ +---------+ +--
        ^      ^                           ^
        |      |                           |
        |      |                           Mandatory Page Break
        |      |
        |      ID header is contained on a single page
        |
        'Beginning Of Stream'

       Figure 1: Example Packet Organization for a Logical Ogg Opus Stream
    */

    fn write_headers(&mut self) -> Result<()> {
        // ID Header
        let mut ogg_id_header = Vec::with_capacity(19);
        {
            let mut header_writer = BufWriter::new(&mut ogg_id_header);
            header_writer.write_all(ID_PAGE_SIGNATURE)?; // Magic Signature 'OpusHead'
            header_writer.write_u8(1)?; // Version //8
            header_writer.write_u8(self.channel_count)?; // Channel count //9
            header_writer.write_u16::<LittleEndian>(DEFAULT_PRE_SKIP)?; // pre-skip //10-11
            header_writer.write_u32::<LittleEndian>(self.sample_rate)?; // original sample rate, any valid sample e.g 48000, //12-15
            header_writer.write_u16::<LittleEndian>(0)?; // output gain // 16-17
            header_writer.write_u8(0)?; // channel map 0 = one stream: mono or stereo, //18
        }

        // Reference: https://tools.ietf.org/html/rfc7845.html#page-6
        // RFC specifies that the ID Header page should have a granule position of 0 and a Header Type set to 2 (StartOfStream)
        self.write_page(
            &Bytes::from(ogg_id_header),
            PAGE_HEADER_TYPE_BEGINNING_OF_STREAM,
            0,
            self.page_index,
        )?;
        self.page_index += 1;

        // Comment Header
        let mut ogg_comment_header = Vec::with_capacity(25);
        {
            let mut header_writer = BufWriter::new(&mut ogg_comment_header);
            header_writer.write_all(COMMENT_PAGE_SIGNATURE)?; // Magic Signature 'OpusTags' //0-7
            header_writer.write_u32::<LittleEndian>(10)?; // Vendor Length //8-11
            header_writer.write_all(b"WebRTC.rs")?; // Vendor name 'WebRTC.rs' //12-20
            header_writer.write_u32::<LittleEndian>(0)?; // User Comment List Length //21-24
        }

        // RFC specifies that the page where the CommentHeader completes should have a granule position of 0
        self.write_page(
            &Bytes::from(ogg_comment_header),
            PAGE_HEADER_TYPE_CONTINUATION_OF_STREAM,
            0,
            self.page_index,
        )?;
        self.page_index += 1;

        Ok(())
    }

    fn write_page(
        &mut self,
        payload: &Bytes,
        header_type: u8,
        granule_pos: u64,
        page_index: u32,
    ) -> Result<()> {
        self.last_payload_size = payload.len();
        self.last_payload = payload.clone();
        let n_segments = (self.last_payload_size + 255 - 1) / 255;

        let mut page =
            Vec::with_capacity(PAGE_HEADER_SIZE + 1 + self.last_payload_size + n_segments);
        {
            let mut header_writer = BufWriter::new(&mut page);
            header_writer.write_all(PAGE_HEADER_SIGNATURE)?; // page headers starts with 'OggS'//0-3
            header_writer.write_u8(0)?; // Version//4
            header_writer.write_u8(header_type)?; // 1 = continuation, 2 = beginning of stream, 4 = end of stream//5
            header_writer.write_u64::<LittleEndian>(granule_pos)?; // granule position //6-13
            header_writer.write_u32::<LittleEndian>(self.serial)?; // Bitstream serial number//14-17
            header_writer.write_u32::<LittleEndian>(page_index)?; // Page sequence number//18-21
            header_writer.write_u32::<LittleEndian>(0)?; //Checksum reserve //22-25
            header_writer.write_u8(n_segments as u8)?; // Number of segments in page //26

            // Filling the segment table with the lacing values.
            // First (n_segments - 1) values will always be 255.
            for _ in 0..n_segments - 1 {
                header_writer.write_u8(255)?;
            }
            // The last value will be the remainder.
            header_writer.write_u8((self.last_payload_size - (n_segments * 255 - 255)) as u8)?;

            header_writer.write_all(payload)?; // inserting at 28th since Segment Table(1) + header length(27)
        }

        let mut checksum = 0u32;
        for v in &page {
            checksum =
                (checksum << 8) ^ self.checksum_table[(((checksum >> 24) as u8) ^ (*v)) as usize];
        }
        page[22..26].copy_from_slice(&checksum.to_le_bytes()); // Checksum - generating for page data and inserting at 22th position into 32 bits

        self.writer.write_all(&page)?;

        Ok(())
    }
}

impl<W: Write + Seek> Writer for OggWriter<W> {
    /// write_rtp adds a new packet and writes the appropriate headers for it
    fn write_rtp(&mut self, packet: &rtp::packet::Packet) -> Result<()> {
        let mut opus_packet = rtp::codecs::opus::OpusPacket;
        let payload = opus_packet.depacketize(&packet.payload)?;

        // Should be equivalent to sample_rate * duration
        if self.previous_timestamp != 1 {
            let increment = packet.header.timestamp - self.previous_timestamp;
            self.previous_granule_position += increment as u64;
        }
        self.previous_timestamp = packet.header.timestamp;

        self.write_page(
            &payload,
            PAGE_HEADER_TYPE_CONTINUATION_OF_STREAM,
            self.previous_granule_position,
            self.page_index,
        )?;
        self.page_index += 1;

        Ok(())
    }

    /// close stops the recording
    fn close(&mut self) -> Result<()> {
        let payload = self.last_payload.clone();
        self.write_page(
            &payload,
            PAGE_HEADER_TYPE_END_OF_STREAM,
            self.previous_granule_position,
            self.page_index - 1,
        )?;

        self.writer.flush()?;
        Ok(())
    }
}
