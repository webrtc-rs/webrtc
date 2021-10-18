use super::*;

/// ChunkType enumerates the three kinds of chunks described in RFC 3611 section 4.1.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ChunkType {
    RunLength = 0,
    BitVector = 1,
    TerminatingNull = 2,
}

/// Chunk as defined in RFC 3611, section 4.1. These represent information
/// about packet losses and packet duplication. They have three representations:
///
/// Run Length Chunk:
///
///   0                   1
///   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  |C|R|        run length         |
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// Bit Vector Chunk:
///
///   0                   1
///   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  |C|        bit vector           |
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// Terminating Null Chunk:
///
///   0                   1
///   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  |0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0|
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Chunk(pub u16);

impl fmt::Display for Chunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.chunk_type() {
            ChunkType::RunLength => {
                let run_type = self.run_type().unwrap_or(0);
                write!(f, "[RunLength type={}, length={}]", run_type, self.value())
            }
            ChunkType::BitVector => write!(f, "[BitVector {:#b}", self.value()),
            ChunkType::TerminatingNull => write!(f, "[TerminatingNull]"),
            //_ => write!(f, "[{:#x}]", self.0),
        }
    }
}
impl Chunk {
    /// chunk_type returns the ChunkType that this Chunk represents
    pub fn chunk_type(&self) -> ChunkType {
        if self.0 == 0 {
            ChunkType::TerminatingNull
        } else if (self.0 >> 15) == 0 {
            ChunkType::RunLength
        } else {
            ChunkType::BitVector
        }
    }

    /// run_type returns the run_type that this Chunk represents. It is
    /// only valid if ChunkType is RunLengthChunkType.
    pub fn run_type(&self) -> error::Result<u8> {
        if self.chunk_type() != ChunkType::RunLength {
            Err(error::Error::WrongChunkType)
        } else {
            Ok((self.0 >> 14) as u8 & 0x01)
        }
    }

    /// value returns the value represented in this Chunk
    pub fn value(&self) -> u16 {
        match self.chunk_type() {
            ChunkType::RunLength => self.0 & 0x3FFF,
            ChunkType::BitVector => self.0 & 0x7FFF,
            ChunkType::TerminatingNull => 0,
            //_ => self.0,
        }
    }
}

/// RleReportBlock defines the common structure used by both
/// Loss RLE report blocks (RFC 3611 §4.1) and Duplicate RLE
/// report blocks (RFC 3611 §4.2).
///
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |  BT = 1 or 2  | rsvd. |   t   |         block length          |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                        ssrc of source                         |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |          begin_seq            |             end_seq           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |          chunk 1              |             chunk 2           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// :                              ...                              :
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |          chunk n-1            |             chunk n           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Default, PartialEq, Clone)]
pub struct RleReportBlock {
    pub xr_header: XRHeader,
    pub t: u8,
    pub ssrc: u32,
    pub begin_seq: u16,
    pub end_seq: u16,
    pub chunks: Vec<Chunk>,
}

impl RleReportBlock {
    fn raw_size(&self) -> usize {
        4 + 1 + 4 + 2 + 2 + self.chunks.len() * 2
    }
}

/// LossRLEReportBlock is used to report information about packet
/// losses, as described in RFC 3611, section 4.1
#[derive(Debug, Default, PartialEq, Clone)]
pub struct LossRLEReportBlock(pub RleReportBlock);

impl fmt::Display for LossRLEReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ReportBlock for LossRLEReportBlock {
    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.0.ssrc]
    }

    fn setup_block_header(&mut self) {
        self.0.xr_header.block_type = ReportBlockType::LossRLE;
        self.0.xr_header.type_specific = self.0.t & 0x0F;
        self.0.xr_header.block_length = (self.0.raw_size() / 4 - 1) as u16;
    }

    fn unpack_block_header(&mut self) {
        self.0.t = (self.0.xr_header.type_specific) & 0x0F;
    }

    fn raw_size(&self) -> usize {
        self.0.raw_size()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn ReportBlock + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<LossRLEReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn ReportBlock + Send + Sync> {
        Box::new(self.clone())
    }
}

/// DuplicateRLEReportBlock is used to report information about packet
/// duplication, as described in RFC 3611, section 4.1
#[derive(Debug, Default, PartialEq, Clone)]
pub struct DuplicateRLEReportBlock(pub RleReportBlock);

impl fmt::Display for DuplicateRLEReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ReportBlock for DuplicateRLEReportBlock {
    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.0.ssrc]
    }

    fn setup_block_header(&mut self) {
        self.0.xr_header.block_type = ReportBlockType::DuplicateRLE;
        self.0.xr_header.type_specific = self.0.t & 0x0F;
        self.0.xr_header.block_length = (self.0.raw_size() / 4 - 1) as u16;
    }

    fn unpack_block_header(&mut self) {
        self.0.t = (self.0.xr_header.type_specific) & 0x0F;
    }

    fn raw_size(&self) -> usize {
        self.0.raw_size()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn ReportBlock + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<DuplicateRLEReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn ReportBlock + Send + Sync> {
        Box::new(self.clone())
    }
}
