#[cfg(test)]
mod h264_reader_test;

use std::fmt;
use std::io::Read;

use bytes::{BufMut, BytesMut};

use crate::error::{Error, Result};

/// NalUnitType is the type of a NAL
/// Enums for NalUnitTypes
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum NalUnitType {
    /// Unspecified
    #[default]
    Unspecified = 0,
    /// Coded slice of a non-IDR picture
    CodedSliceNonIdr = 1,
    /// Coded slice data partition A
    CodedSliceDataPartitionA = 2,
    /// Coded slice data partition B
    CodedSliceDataPartitionB = 3,
    /// Coded slice data partition C
    CodedSliceDataPartitionC = 4,
    /// Coded slice of an IDR picture
    CodedSliceIdr = 5,
    /// Supplemental enhancement information (SEI)
    SEI = 6,
    /// Sequence parameter set
    SPS = 7,
    /// Picture parameter set
    PPS = 8,
    /// Access unit delimiter
    AUD = 9,
    /// End of sequence
    EndOfSequence = 10,
    /// End of stream
    EndOfStream = 11,
    /// Filler data
    Filler = 12,
    /// Sequence parameter set extension
    SpsExt = 13,
    /// Coded slice of an auxiliary coded picture without partitioning
    CodedSliceAux = 19,
    ///Reserved
    Reserved,
    // 14..18                                            // Reserved
    // 20..23                                            // Reserved
    // 24..31                                            // Unspecified
}

impl fmt::Display for NalUnitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            NalUnitType::Unspecified => "Unspecified",
            NalUnitType::CodedSliceNonIdr => "CodedSliceNonIdr",
            NalUnitType::CodedSliceDataPartitionA => "CodedSliceDataPartitionA",
            NalUnitType::CodedSliceDataPartitionB => "CodedSliceDataPartitionB",
            NalUnitType::CodedSliceDataPartitionC => "CodedSliceDataPartitionC",
            NalUnitType::CodedSliceIdr => "CodedSliceIdr",
            NalUnitType::SEI => "SEI",
            NalUnitType::SPS => "SPS",
            NalUnitType::PPS => "PPS",
            NalUnitType::AUD => "AUD",
            NalUnitType::EndOfSequence => "EndOfSequence",
            NalUnitType::EndOfStream => "EndOfStream",
            NalUnitType::Filler => "Filler",
            NalUnitType::SpsExt => "SpsExt",
            NalUnitType::CodedSliceAux => "NalUnitTypeCodedSliceAux",
            _ => "Reserved",
        };
        write!(f, "{}({})", s, *self as u8)
    }
}

impl From<u8> for NalUnitType {
    fn from(v: u8) -> Self {
        match v {
            0 => NalUnitType::Unspecified,
            1 => NalUnitType::CodedSliceNonIdr,
            2 => NalUnitType::CodedSliceDataPartitionA,
            3 => NalUnitType::CodedSliceDataPartitionB,
            4 => NalUnitType::CodedSliceDataPartitionC,
            5 => NalUnitType::CodedSliceIdr,
            6 => NalUnitType::SEI,
            7 => NalUnitType::SPS,
            8 => NalUnitType::PPS,
            9 => NalUnitType::AUD,
            10 => NalUnitType::EndOfSequence,
            11 => NalUnitType::EndOfStream,
            12 => NalUnitType::Filler,
            13 => NalUnitType::SpsExt,
            19 => NalUnitType::CodedSliceAux,
            _ => NalUnitType::Reserved,
        }
    }
}

/// NAL H.264 Network Abstraction Layer
pub struct NAL {
    pub picture_order_count: u32,

    /// NAL header
    pub forbidden_zero_bit: bool,
    pub ref_idc: u8,
    pub unit_type: NalUnitType,

    /// header byte + rbsp
    pub data: BytesMut,
}

impl NAL {
    fn new(data: BytesMut) -> Self {
        NAL {
            picture_order_count: 0,
            forbidden_zero_bit: false,
            ref_idc: 0,
            unit_type: NalUnitType::Unspecified,
            data,
        }
    }

    fn parse_header(&mut self) {
        let first_byte = self.data[0];
        self.forbidden_zero_bit = ((first_byte & 0x80) >> 7) == 1; // 0x80 = 0b10000000
        self.ref_idc = (first_byte & 0x60) >> 5; // 0x60 = 0b01100000
        self.unit_type = NalUnitType::from(first_byte & 0x1F); // 0x1F = 0b00011111
    }
}

const NAL_PREFIX_3BYTES: [u8; 3] = [0, 0, 1];
const NAL_PREFIX_4BYTES: [u8; 4] = [0, 0, 0, 1];

/// Wrapper class around reading buffer
struct ReadBuffer {
    buffer: Box<[u8]>,
    read_end: usize,
    filled_end: usize,
}

impl ReadBuffer {
    fn new(capacity: usize) -> ReadBuffer {
        Self {
            buffer: vec![0u8; capacity].into_boxed_slice(),
            read_end: 0,
            filled_end: 0,
        }
    }

    #[inline]
    fn in_buffer(&self) -> usize {
        self.filled_end - self.read_end
    }

    fn consume(&mut self, consume: usize) -> &[u8] {
        debug_assert!(self.read_end + consume <= self.filled_end);
        let result = &self.buffer[self.read_end..][..consume];
        self.read_end += consume;
        result
    }

    pub(crate) fn fill_buffer(&mut self, reader: &mut impl Read) -> Result<()> {
        debug_assert_eq!(self.read_end, self.filled_end);

        self.read_end = 0;
        self.filled_end = reader.read(&mut self.buffer)?;

        Ok(())
    }
}

/// H264Reader reads data from stream and constructs h264 nal units
pub struct H264Reader<R: Read> {
    reader: R,
    // reading buffers
    buffer: ReadBuffer,
    // for reading
    nal_prefix_parsed: bool,
    count_of_consecutive_zero_bytes: usize,
    nal_buffer: BytesMut,
}

impl<R: Read> H264Reader<R> {
    /// new creates new `H264Reader` with `capacity` sized read buffer.
    pub fn new(reader: R, capacity: usize) -> H264Reader<R> {
        H264Reader {
            reader,
            nal_prefix_parsed: false,
            buffer: ReadBuffer::new(capacity),
            count_of_consecutive_zero_bytes: 0,
            nal_buffer: BytesMut::new(),
        }
    }

    fn read4(&mut self) -> Result<([u8; 4], usize)> {
        let mut result = [0u8; 4];
        let mut result_filled = 0;
        loop {
            let in_buffer = self.buffer.in_buffer();

            if in_buffer + result_filled >= 4 {
                let consume = 4 - result_filled;
                result[result_filled..].copy_from_slice(self.buffer.consume(consume));
                return Ok((result, 4));
            }

            result[result_filled..][..in_buffer].copy_from_slice(self.buffer.consume(in_buffer));
            result_filled += in_buffer;

            self.buffer.fill_buffer(&mut self.reader)?;

            if self.buffer.in_buffer() == 0 {
                return Ok((result, result_filled));
            }
        }
    }

    fn read1(&mut self) -> Result<Option<u8>> {
        if self.buffer.in_buffer() == 0 {
            self.buffer.fill_buffer(&mut self.reader)?;

            if self.buffer.in_buffer() == 0 {
                return Ok(None);
            }
        }

        Ok(Some(self.buffer.consume(1)[0]))
    }

    fn bit_stream_starts_with_h264prefix(&mut self) -> Result<usize> {
        let (prefix_buffer, n) = self.read4()?;

        if n == 0 {
            return Err(Error::ErrIoEOF);
        }

        if n < 3 {
            return Err(Error::ErrDataIsNotH264Stream);
        }

        let nal_prefix3bytes_found = NAL_PREFIX_3BYTES[..] == prefix_buffer[..3];
        if n == 3 {
            if nal_prefix3bytes_found {
                return Err(Error::ErrIoEOF);
            }
            return Err(Error::ErrDataIsNotH264Stream);
        }

        // n == 4
        if nal_prefix3bytes_found {
            self.nal_buffer.put_u8(prefix_buffer[3]);
            return Ok(3);
        }

        let nal_prefix4bytes_found = NAL_PREFIX_4BYTES[..] == prefix_buffer;
        if nal_prefix4bytes_found {
            Ok(4)
        } else {
            Err(Error::ErrDataIsNotH264Stream)
        }
    }

    /// next_nal reads from stream and returns then next NAL,
    /// and an error if there is incomplete frame data.
    /// Returns all nil values when no more NALs are available.
    pub fn next_nal(&mut self) -> Result<NAL> {
        if !self.nal_prefix_parsed {
            self.bit_stream_starts_with_h264prefix()?;

            self.nal_prefix_parsed = true;
        }

        loop {
            let Some(read_byte) = self.read1()? else {
                break;
            };

            let nal_found = self.process_byte(read_byte);
            if nal_found {
                let nal_unit_type = NalUnitType::from(self.nal_buffer[0] & 0x1F);
                if nal_unit_type == NalUnitType::SEI {
                    self.nal_buffer.clear();
                    continue;
                } else {
                    break;
                }
            }

            self.nal_buffer.put_u8(read_byte);
        }

        if self.nal_buffer.is_empty() {
            return Err(Error::ErrIoEOF);
        }

        let mut nal = NAL::new(self.nal_buffer.split());
        nal.parse_header();

        Ok(nal)
    }

    fn process_byte(&mut self, read_byte: u8) -> bool {
        let mut nal_found = false;

        match read_byte {
            0 => {
                self.count_of_consecutive_zero_bytes += 1;
            }
            1 => {
                if self.count_of_consecutive_zero_bytes >= 2 {
                    let count_of_consecutive_zero_bytes_in_prefix =
                        if self.count_of_consecutive_zero_bytes > 2 {
                            3
                        } else {
                            2
                        };
                    let nal_unit_length =
                        self.nal_buffer.len() - count_of_consecutive_zero_bytes_in_prefix;
                    if nal_unit_length > 0 {
                        let _ = self.nal_buffer.split_off(nal_unit_length);
                        nal_found = true;
                    }
                }
                self.count_of_consecutive_zero_bytes = 0;
            }
            _ => {
                self.count_of_consecutive_zero_bytes = 0;
            }
        }

        nal_found
    }
}
