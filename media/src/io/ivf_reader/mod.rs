#[cfg(test)]
mod ivf_reader_test;

use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};
use bytes::BytesMut;

use crate::error::{Error, Result};
use crate::io::ResetFn;

pub const IVF_FILE_HEADER_SIGNATURE: &[u8] = b"DKIF";
pub const IVF_FILE_HEADER_SIZE: usize = 32;
pub const IVF_FRAME_HEADER_SIZE: usize = 12;

/// IVFFileHeader 32-byte header for IVF files
/// <https://wiki.multimedia.cx/index.php/IVF>
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct IVFFileHeader {
    pub signature: [u8; 4],        // 0-3
    pub version: u16,              // 4-5
    pub header_size: u16,          // 6-7
    pub four_cc: [u8; 4],          // 8-11
    pub width: u16,                // 12-13
    pub height: u16,               // 14-15
    pub timebase_denominator: u32, // 16-19
    pub timebase_numerator: u32,   // 20-23
    pub num_frames: u32,           // 24-27
    pub unused: u32,               // 28-31
}

/// IVFFrameHeader 12-byte header for IVF frames
/// <https://wiki.multimedia.cx/index.php/IVF>
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct IVFFrameHeader {
    pub frame_size: u32, // 0-3
    pub timestamp: u64,  // 4-11
}

/// IVFReader is used to read IVF files and return frame payloads
pub struct IVFReader<R: Read> {
    reader: R,
    bytes_read: usize,
}

impl<R: Read> IVFReader<R> {
    /// new returns a new IVF reader and IVF file header
    /// with an io.Reader input
    pub fn new(reader: R) -> Result<(IVFReader<R>, IVFFileHeader)> {
        let mut r = IVFReader {
            reader,
            bytes_read: 0,
        };

        let header = r.parse_file_header()?;

        Ok((r, header))
    }

    /// reset_reader resets the internal stream of IVFReader. This is useful
    /// for live streams, where the end of the file might be read without the
    /// data being finished.
    pub fn reset_reader(&mut self, mut reset: ResetFn<R>) {
        self.reader = reset(self.bytes_read);
    }

    /// parse_next_frame reads from stream and returns IVF frame payload, header,
    /// and an error if there is incomplete frame data.
    /// Returns all nil values when no more frames are available.
    pub fn parse_next_frame(&mut self) -> Result<(BytesMut, IVFFrameHeader)> {
        let frame_size = self.reader.read_u32::<LittleEndian>()?;
        let timestamp = self.reader.read_u64::<LittleEndian>()?;
        let header = IVFFrameHeader {
            frame_size,
            timestamp,
        };

        let mut payload = BytesMut::with_capacity(header.frame_size as usize);
        payload.resize(header.frame_size as usize, 0);
        self.reader.read_exact(&mut payload)?;

        self.bytes_read += IVF_FRAME_HEADER_SIZE + header.frame_size as usize;

        Ok((payload, header))
    }

    /// parse_file_header reads 32 bytes from stream and returns
    /// IVF file header. This is always called before parse_next_frame()
    fn parse_file_header(&mut self) -> Result<IVFFileHeader> {
        let mut signature = [0u8; 4];
        let mut four_cc = [0u8; 4];

        self.reader.read_exact(&mut signature)?;
        let version = self.reader.read_u16::<LittleEndian>()?;
        let header_size = self.reader.read_u16::<LittleEndian>()?;
        self.reader.read_exact(&mut four_cc)?;
        let width = self.reader.read_u16::<LittleEndian>()?;
        let height = self.reader.read_u16::<LittleEndian>()?;
        let timebase_denominator = self.reader.read_u32::<LittleEndian>()?;
        let timebase_numerator = self.reader.read_u32::<LittleEndian>()?;
        let num_frames = self.reader.read_u32::<LittleEndian>()?;
        let unused = self.reader.read_u32::<LittleEndian>()?;

        let header = IVFFileHeader {
            signature,
            version,
            header_size,
            four_cc,
            width,
            height,
            timebase_denominator,
            timebase_numerator,
            num_frames,
            unused,
        };

        if header.signature != IVF_FILE_HEADER_SIGNATURE {
            return Err(Error::ErrSignatureMismatch);
        } else if header.version != 0 {
            return Err(Error::ErrUnknownIVFVersion);
        }

        self.bytes_read += IVF_FILE_HEADER_SIZE;

        Ok(header)
    }
}
