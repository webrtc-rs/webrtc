#[cfg(test)]
mod ogg_reader_test;

use std::io::{Cursor, Read};

use byteorder::{LittleEndian, ReadBytesExt};
use bytes::BytesMut;

use crate::error::{Error, Result};
use crate::io::ResetFn;

pub const PAGE_HEADER_TYPE_CONTINUATION_OF_STREAM: u8 = 0x00;
pub const PAGE_HEADER_TYPE_BEGINNING_OF_STREAM: u8 = 0x02;
pub const PAGE_HEADER_TYPE_END_OF_STREAM: u8 = 0x04;
pub const DEFAULT_PRE_SKIP: u16 = 3840; // 3840 recommended in the RFC
pub const PAGE_HEADER_SIGNATURE: &[u8] = b"OggS";
pub const ID_PAGE_SIGNATURE: &[u8] = b"OpusHead";
pub const COMMENT_PAGE_SIGNATURE: &[u8] = b"OpusTags";
pub const PAGE_HEADER_SIZE: usize = 27;
pub const ID_PAGE_PAYLOAD_SIZE: usize = 19;

/// OggReader is used to read Ogg files and return page payloads
pub struct OggReader<R: Read> {
    reader: R,
    bytes_read: usize,
    checksum_table: [u32; 256],
    do_checksum: bool,
}

/// OggHeader is the metadata from the first two pages
/// in the file (ID and Comment)
/// <https://tools.ietf.org/html/rfc7845.html#section-3>
pub struct OggHeader {
    pub channel_map: u8,
    pub channels: u8,
    pub output_gain: u16,
    pub pre_skip: u16,
    pub sample_rate: u32,
    pub version: u8,
}

/// OggPageHeader is the metadata for a Page
/// Pages are the fundamental unit of multiplexing in an Ogg stream
/// <https://tools.ietf.org/html/rfc7845.html#section-1>
pub struct OggPageHeader {
    pub granule_position: u64,

    sig: [u8; 4],
    version: u8,
    header_type: u8,
    serial: u32,
    index: u32,
    segments_count: u8,
}

impl<R: Read> OggReader<R> {
    /// new returns a new Ogg reader and Ogg header
    /// with an io.Reader input
    pub fn new(reader: R, do_checksum: bool) -> Result<(OggReader<R>, OggHeader)> {
        let mut r = OggReader {
            reader,
            bytes_read: 0,
            checksum_table: generate_checksum_table(),
            do_checksum,
        };

        let header = r.read_headers()?;

        Ok((r, header))
    }

    fn read_headers(&mut self) -> Result<OggHeader> {
        let (payload, page_header) = self.parse_next_page()?;

        if page_header.sig != PAGE_HEADER_SIGNATURE {
            return Err(Error::ErrBadIDPageSignature);
        }

        if page_header.header_type != PAGE_HEADER_TYPE_BEGINNING_OF_STREAM {
            return Err(Error::ErrBadIDPageType);
        }

        if payload.len() != ID_PAGE_PAYLOAD_SIZE {
            return Err(Error::ErrBadIDPageLength);
        }

        let s = &payload[..8];
        if s != ID_PAGE_SIGNATURE {
            return Err(Error::ErrBadIDPagePayloadSignature);
        }

        let mut reader = Cursor::new(&payload[8..]);
        let version = reader.read_u8()?; //8
        let channels = reader.read_u8()?; //9
        let pre_skip = reader.read_u16::<LittleEndian>()?; //10-11
        let sample_rate = reader.read_u32::<LittleEndian>()?; //12-15
        let output_gain = reader.read_u16::<LittleEndian>()?; //16-17
        let channel_map = reader.read_u8()?; //18

        Ok(OggHeader {
            channel_map,
            channels,
            output_gain,
            pre_skip,
            sample_rate,
            version,
        })
    }

    // parse_next_page reads from stream and returns Ogg page payload, header,
    // and an error if there is incomplete page data.
    pub fn parse_next_page(&mut self) -> Result<(BytesMut, OggPageHeader)> {
        let mut h = [0u8; PAGE_HEADER_SIZE];
        self.reader.read_exact(&mut h)?;

        let mut head_reader = Cursor::new(h);
        let mut sig = [0u8; 4]; //0-3
        head_reader.read_exact(&mut sig)?;
        let version = head_reader.read_u8()?; //4
        let header_type = head_reader.read_u8()?; //5
        let granule_position = head_reader.read_u64::<LittleEndian>()?; //6-13
        let serial = head_reader.read_u32::<LittleEndian>()?; //14-17
        let index = head_reader.read_u32::<LittleEndian>()?; //18-21
        let checksum = head_reader.read_u32::<LittleEndian>()?; //22-25
        let segments_count = head_reader.read_u8()?; //26

        let mut size_buffer = vec![0u8; segments_count as usize];
        self.reader.read_exact(&mut size_buffer)?;

        let mut payload_size = 0usize;
        for s in &size_buffer {
            payload_size += *s as usize;
        }

        let mut payload = BytesMut::with_capacity(payload_size);
        payload.resize(payload_size, 0);
        self.reader.read_exact(&mut payload)?;

        if self.do_checksum {
            let mut sum = 0;

            for (index, v) in h.iter().enumerate() {
                // Don't include expected checksum in our generation
                if index > 21 && index < 26 {
                    sum = self.update_checksum(0, sum);
                    continue;
                }
                sum = self.update_checksum(*v, sum);
            }

            for v in &size_buffer {
                sum = self.update_checksum(*v, sum);
            }
            for v in &payload[..] {
                sum = self.update_checksum(*v, sum);
            }

            if sum != checksum {
                return Err(Error::ErrChecksumMismatch);
            }
        }

        let page_header = OggPageHeader {
            granule_position,
            sig,
            version,
            header_type,
            serial,
            index,
            segments_count,
        };

        Ok((payload, page_header))
    }

    /// reset_reader resets the internal stream of OggReader. This is useful
    /// for live streams, where the end of the file might be read without the
    /// data being finished.
    pub fn reset_reader(&mut self, mut reset: ResetFn<R>) {
        self.reader = reset(self.bytes_read);
    }

    fn update_checksum(&self, v: u8, sum: u32) -> u32 {
        (sum << 8) ^ self.checksum_table[(((sum >> 24) as u8) ^ v) as usize]
    }
}

pub(crate) fn generate_checksum_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    const POLY: u32 = 0x04c11db7;

    for (i, t) in table.iter_mut().enumerate() {
        let mut r = (i as u32) << 24;
        for _ in 0..8 {
            if (r & 0x80000000) != 0 {
                r = (r << 1) ^ POLY;
            } else {
                r <<= 1;
            }
        }
        *t = r;
    }
    table
}
