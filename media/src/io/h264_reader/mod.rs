#[cfg(test)]
mod h264_reader_test;

use std::fmt;
use std::io::Read;

use bytes::{BufMut, Bytes, BytesMut};

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

/// H264Reader reads data from stream and constructs h264 nal units
pub struct H264Reader<R: Read> {
    reader: R,
    nal_buffer: BytesMut,
    count_of_consecutive_zero_bytes: usize,
    nal_prefix_parsed: bool,
    read_buffer: Vec<u8>,
    temp_buf: Vec<u8>,
}

impl<R: Read> H264Reader<R> {
    /// new creates new `H264Reader` with `capacity` sized read buffer.
    pub fn new(reader: R, capacity: usize) -> H264Reader<R> {
        H264Reader {
            reader,
            nal_buffer: BytesMut::new(),
            count_of_consecutive_zero_bytes: 0,
            nal_prefix_parsed: false,
            read_buffer: vec![],
            temp_buf: vec![0u8; capacity],
        }
    }

    fn read(&mut self, num_to_read: usize) -> Result<Bytes> {
        let buf = &mut self.temp_buf;
        while self.read_buffer.len() < num_to_read {
            let n = match self.reader.read(buf) {
                Ok(n) => {
                    if n == 0 {
                        break;
                    }
                    n
                }
                Err(e) => return Err(Error::Io(e.into())),
            };

            self.read_buffer.extend_from_slice(&buf[0..n]);
        }

        let num_should_read = if num_to_read <= self.read_buffer.len() {
            num_to_read
        } else {
            self.read_buffer.len()
        };

        Ok(Bytes::from(
            self.read_buffer
                .drain(..num_should_read)
                .collect::<Vec<u8>>(),
        ))
    }

    fn bit_stream_starts_with_h264prefix(&mut self) -> Result<usize> {
        let prefix_buffer = self.read(4)?;

        let n = prefix_buffer.len();
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
            let buffer = self.read(1)?;
            let n = buffer.len();

            if n != 1 {
                break;
            }
            let read_byte = buffer[0];
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
