use super::*;

const RLE_REPORT_BLOCK_MIN_LENGTH: u16 = 8;

/// ChunkType enumerates the three kinds of chunks described in RFC 3611 section 4.1.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct RLEReportBlock {
    //not included in marshal/unmarshal
    pub is_loss_rle: bool,
    pub t: u8,

    //marshal/unmarshal
    pub ssrc: u32,
    pub begin_seq: u16,
    pub end_seq: u16,
    pub chunks: Vec<Chunk>,
}

/// LossRLEReportBlock is used to report information about packet
/// losses, as described in RFC 3611, section 4.1
/// make sure to set is_loss_rle = true
pub type LossRLEReportBlock = RLEReportBlock;

/// DuplicateRLEReportBlock is used to report information about packet
/// duplication, as described in RFC 3611, section 4.1
/// make sure to set is_loss_rle = false
pub type DuplicateRLEReportBlock = RLEReportBlock;

impl RLEReportBlock {
    pub fn xr_header(&self) -> XRHeader {
        XRHeader {
            block_type: if self.is_loss_rle {
                BlockType::LossRLE
            } else {
                BlockType::DuplicateRLE
            },
            type_specific: self.t & 0x0F,
            block_length: (self.raw_size() / 4 - 1) as u16,
        }
    }
}

impl fmt::Display for RLEReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Packet for RLEReportBlock {
    fn header(&self) -> Header {
        Header::default()
    }

    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.ssrc]
    }

    fn raw_size(&self) -> usize {
        XR_HEADER_LENGTH + RLE_REPORT_BLOCK_MIN_LENGTH as usize + self.chunks.len() * 2
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<RLEReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for RLEReportBlock {
    fn marshal_size(&self) -> usize {
        self.raw_size()
    }
}

impl Marshal for RLEReportBlock {
    /// marshal_to encodes the RLEReportBlock in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if buf.remaining_mut() < self.marshal_size() {
            return Err(error::Error::BufferTooShort.into());
        }

        let h = self.xr_header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put_u32(self.ssrc);
        buf.put_u16(self.begin_seq);
        buf.put_u16(self.end_seq);
        for chunk in &self.chunks {
            buf.put_u16(chunk.0);
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for RLEReportBlock {
    /// Unmarshal decodes the RLEReportBlock from binary
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        if raw_packet.remaining() < XR_HEADER_LENGTH {
            return Err(error::Error::PacketTooShort.into());
        }

        let xr_header = XRHeader::unmarshal(raw_packet)?;
        let block_length = xr_header.block_length * 4;
        if block_length < RLE_REPORT_BLOCK_MIN_LENGTH
            || (block_length - RLE_REPORT_BLOCK_MIN_LENGTH) % 2 != 0
            || raw_packet.remaining() < block_length as usize
        {
            return Err(error::Error::PacketTooShort.into());
        }

        let is_loss_rle = xr_header.block_type == BlockType::LossRLE;
        let t = xr_header.type_specific & 0x0F;

        let ssrc = raw_packet.get_u32();
        let begin_seq = raw_packet.get_u16();
        let end_seq = raw_packet.get_u16();

        let remaining = block_length - RLE_REPORT_BLOCK_MIN_LENGTH;
        let mut chunks = vec![];
        for _ in 0..remaining / 2 {
            chunks.push(Chunk(raw_packet.get_u16()));
        }

        Ok(RLEReportBlock {
            is_loss_rle,
            t,
            ssrc,
            begin_seq,
            end_seq,
            chunks,
        })
    }
}
