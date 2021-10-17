#[cfg(test)]
mod extended_report_test;

use crate::error;
/*
use util::marshal::{Marshal, MarshalSize, Unmarshal};
use crate::header::{Header, PacketType, HEADER_LENGTH, SSRC_LENGTH};
use crate::packet::Packet;
use crate::util::get_padding_size;
use bytes::{Buf, BufMut, Bytes};
*/

use std::any::Any;
use std::fmt;

type Result<T> = std::result::Result<T, util::Error>;

/// ReportBlock represents a single report within an ExtendedReport
/// packet
pub trait ReportBlock: fmt::Display + fmt::Debug {
    fn destination_ssrc(&self) -> Vec<u32>;
    fn setup_block_header(&mut self);
    fn unpack_block_header(&mut self);
    fn raw_size(&self) -> usize;
    fn as_any(&self) -> &(dyn Any + Send + Sync);
    fn equal(&self, other: &(dyn ReportBlock + Send + Sync)) -> bool;
    fn cloned(&self) -> Box<dyn ReportBlock + Send + Sync>;
}

impl PartialEq for dyn ReportBlock + Send + Sync {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Clone for Box<dyn ReportBlock + Send + Sync> {
    fn clone(&self) -> Box<dyn ReportBlock + Send + Sync> {
        self.cloned()
    }
}

/// TypeSpecificField as described in RFC 3611 section 4.5. In typical
/// cases, users of ExtendedReports shouldn't need to access this,
/// and should instead use the corresponding fields in the actual
/// report blocks themselves.
pub type TypeSpecificField = u8;

/// XRHeader defines the common fields that must appear at the start
/// of each report block. In typical cases, users of ExtendedReports
/// shouldn't need to access this. For locally-constructed report
/// blocks, these values will not be accurate until the corresponding
/// packet is marshaled.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct XRHeader {
    pub block_type: ReportBlockType,
    pub type_specific: TypeSpecificField,
    pub block_length: u16,
}

/// BlockTypeType specifies the type of report in a report block
/// Extended Report block types from RFC 3611.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ReportBlockType {
    Unspecified = 0,
    LossRLE = 1,               // RFC 3611, section 4.1
    DuplicateRLE = 2,          // RFC 3611, section 4.2
    PacketReceiptTimes = 3,    // RFC 3611, section 4.3
    ReceiverReferenceTime = 4, // RFC 3611, section 4.4
    DLRR = 5,                  // RFC 3611, section 4.5
    StatisticsSummary = 6,     // RFC 3611, section 4.6
    VoIPMetrics = 7,           // RFC 3611, section 4.7
}

impl Default for ReportBlockType {
    fn default() -> Self {
        ReportBlockType::Unspecified
    }
}

impl From<u8> for ReportBlockType {
    fn from(v: u8) -> Self {
        match v {
            1 => ReportBlockType::LossRLE,
            2 => ReportBlockType::DuplicateRLE,
            3 => ReportBlockType::PacketReceiptTimes,
            4 => ReportBlockType::ReceiverReferenceTime,
            5 => ReportBlockType::DLRR,
            6 => ReportBlockType::StatisticsSummary,
            7 => ReportBlockType::VoIPMetrics,
            _ => ReportBlockType::Unspecified,
        }
    }
}

/// converts the Extended report block types into readable strings
impl fmt::Display for ReportBlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ReportBlockType::LossRLE => "LossRLEReportBlockType",
            ReportBlockType::DuplicateRLE => "DuplicateRLEReportBlockType",
            ReportBlockType::PacketReceiptTimes => "PacketReceiptTimesReportBlockType",
            ReportBlockType::ReceiverReferenceTime => "ReceiverReferenceTimeReportBlockType",
            ReportBlockType::DLRR => "DLRRReportBlockType",
            ReportBlockType::StatisticsSummary => "StatisticsSummaryReportBlockType",
            ReportBlockType::VoIPMetrics => "VoIPMetricsReportBlockType",
            _ => "Unspecified",
        };
        write!(f, "{}", s)
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

/// PacketReceiptTimesReportBlock represents a Packet Receipt Times
/// report block, as described in RFC 3611 section 4.3.
///
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |     BT=3      | rsvd. |   t   |         block length          |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                        ssrc of source                         |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |          begin_seq            |             end_seq           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |       Receipt time of packet begin_seq                        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |       Receipt time of packet (begin_seq + 1) mod 65536        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// :                              ...                              :
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |       Receipt time of packet (end_seq - 1) mod 65536          |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Default, PartialEq, Clone)]
pub struct PacketReceiptTimesReportBlock {
    pub xr_header: XRHeader,
    pub t: u8,
    pub ssrc: u32,
    pub begin_seq: u16,
    pub end_seq: u16,
    pub receipt_time: Vec<u32>,
}

impl fmt::Display for PacketReceiptTimesReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ReportBlock for PacketReceiptTimesReportBlock {
    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.ssrc]
    }

    fn setup_block_header(&mut self) {
        self.xr_header.block_type = ReportBlockType::PacketReceiptTimes;
        self.xr_header.type_specific = self.t & 0x0F;
        self.xr_header.block_length = (self.raw_size() / 4 - 1) as u16;
    }

    fn unpack_block_header(&mut self) {
        self.t = (self.xr_header.type_specific) & 0x0F;
    }

    fn raw_size(&self) -> usize {
        4 + 1 + 4 + 2 + 2 + self.receipt_time.len() * 4
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn ReportBlock + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<PacketReceiptTimesReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn ReportBlock + Send + Sync> {
        Box::new(self.clone())
    }
}

/// ReceiverReferenceTimeReportBlock encodes a Receiver Reference Time
/// report block as described in RFC 3611 section 4.4.
///
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |     BT=4      |   reserved    |       block length = 2        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |              NTP timestamp, most significant word             |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |             NTP timestamp, least significant word             |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Default, PartialEq, Clone)]
pub struct ReceiverReferenceTimeReportBlock {
    pub xr_header: XRHeader,
    pub ntp_timestamp: u64,
}

impl fmt::Display for ReceiverReferenceTimeReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ReportBlock for ReceiverReferenceTimeReportBlock {
    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn setup_block_header(&mut self) {
        self.xr_header.block_type = ReportBlockType::ReceiverReferenceTime;
        self.xr_header.type_specific = 0;
        self.xr_header.block_length = (self.raw_size() / 4 - 1) as u16;
    }

    fn unpack_block_header(&mut self) {}

    fn raw_size(&self) -> usize {
        4 + 8
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn ReportBlock + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<ReceiverReferenceTimeReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn ReportBlock + Send + Sync> {
        Box::new(self.clone())
    }
}

/// DLRRReportBlock encodes a DLRR Report Block as described in
/// RFC 3611 section 4.5.
///
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |     BT=5      |   reserved    |         block length          |
/// +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
/// |                 SSRC_1 (ssrc of first receiver)               | sub-
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ block
/// |                         last RR (LRR)                         |   1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                   delay since last RR (DLRR)                  |
/// +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
/// |                 SSRC_2 (ssrc of second receiver)              | sub-
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ block
/// :                               ...                             :   2
/// +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
#[derive(Debug, Default, PartialEq, Clone)]
pub struct DLRRReportBlock {
    pub xr_header: XRHeader,
    pub reports: Vec<DLRRReport>,
}

impl fmt::Display for DLRRReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// DLRRReport encodes a single report inside a DLRRReportBlock.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct DLRRReport {
    pub ssrc: u32,
    pub last_rr: u32,
    pub dlrr: u32,
}

impl fmt::Display for DLRRReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ReportBlock for DLRRReportBlock {
    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        let mut ssrc = Vec::with_capacity(self.reports.len());
        for r in &self.reports {
            ssrc.push(r.ssrc);
        }
        ssrc
    }

    fn setup_block_header(&mut self) {
        self.xr_header.block_type = ReportBlockType::DLRR;
        self.xr_header.type_specific = 0;
        self.xr_header.block_length = (self.raw_size() / 4 - 1) as u16;
    }

    fn unpack_block_header(&mut self) {}

    fn raw_size(&self) -> usize {
        4 + self.reports.len() * 4 * 3
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn ReportBlock + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<DLRRReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn ReportBlock + Send + Sync> {
        Box::new(self.clone())
    }
}

/// StatisticsSummaryReportBlock encodes a Statistics Summary Report
/// Block as described in RFC 3611, section 4.6.
///
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |     BT=6      |L|D|J|ToH|rsvd.|       block length = 9        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                        ssrc of source                         |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |          begin_seq            |             end_seq           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                        lost_packets                           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                        dup_packets                            |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                         min_jitter                            |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                         max_jitter                            |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                         mean_jitter                           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                         dev_jitter                            |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// | min_ttl_or_hl | max_ttl_or_hl |mean_ttl_or_hl | dev_ttl_or_hl |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Default, PartialEq, Clone)]
pub struct StatisticsSummaryReportBlock {
    pub xr_header: XRHeader,
    pub loss_reports: bool,
    pub duplicate_reports: bool,
    pub jitter_reports: bool,
    pub ttl_or_hop_limit: TTLorHopLimitType,
    pub ssrc: u32,
    pub begin_seq: u16,
    pub end_seq: u16,
    pub lost_packets: u32,
    pub dup_packets: u32,
    pub min_jitter: u32,
    pub max_jitter: u32,
    pub mean_jitter: u32,
    pub dev_jitter: u32,
    pub min_ttl_or_hl: u8,
    pub max_ttl_or_hl: u8,
    pub mean_ttl_or_hl: u8,
    pub dev_ttl_or_hl: u8,
}

impl fmt::Display for StatisticsSummaryReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// TTLorHopLimitType encodes values for the ToH field in
/// a StatisticsSummaryReportBlock
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TTLorHopLimitType {
    Missing = 0,
    IPv4 = 1,
    IPv6 = 2,
}

impl Default for TTLorHopLimitType {
    fn default() -> Self {
        TTLorHopLimitType::Missing
    }
}

impl From<u8> for TTLorHopLimitType {
    fn from(v: u8) -> Self {
        match v {
            1 => TTLorHopLimitType::IPv4,
            2 => TTLorHopLimitType::IPv4,
            _ => TTLorHopLimitType::Missing,
        }
    }
}

impl fmt::Display for TTLorHopLimitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            TTLorHopLimitType::Missing => "[ToH Missing]",
            TTLorHopLimitType::IPv4 => "[ToH = IPv4]",
            TTLorHopLimitType::IPv6 => "[ToH = IPv6]",
        };
        write!(f, "{}", s)
    }
}

impl ReportBlock for StatisticsSummaryReportBlock {
    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.ssrc]
    }

    fn setup_block_header(&mut self) {
        self.xr_header.block_type = ReportBlockType::StatisticsSummary;
        self.xr_header.type_specific = 0x00;
        if self.loss_reports {
            self.xr_header.type_specific |= 0x80;
        }
        if self.duplicate_reports {
            self.xr_header.type_specific |= 0x40;
        }
        if self.jitter_reports {
            self.xr_header.type_specific |= 0x20;
        }
        self.xr_header.type_specific |= (self.ttl_or_hop_limit as u8 & 0x03) << 3;
        self.xr_header.block_length = (self.raw_size() / 4 - 1) as u16;
    }

    fn unpack_block_header(&mut self) {
        self.loss_reports = self.xr_header.type_specific & 0x80 != 0;
        self.duplicate_reports = self.xr_header.type_specific & 0x40 != 0;
        self.jitter_reports = self.xr_header.type_specific & 0x20 != 0;
        self.ttl_or_hop_limit = ((self.xr_header.type_specific & 0x18) >> 3).into();
    }

    fn raw_size(&self) -> usize {
        4 + 3 + 1 + 4 + 2 * 2 + 4 * 6 + 4
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn ReportBlock + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<StatisticsSummaryReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn ReportBlock + Send + Sync> {
        Box::new(self.clone())
    }
}

/// VoIPMetricsReportBlock encodes a VoIP Metrics Report Block as described
/// in RFC 3611, section 4.7.
///
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |     BT=7      |   reserved    |       block length = 8        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                        ssrc of source                         |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |   loss rate   | discard rate  | burst density |  gap density  |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |       burst duration          |         gap duration          |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |     round trip delay          |       end system delay        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// | signal level  |  noise level  |     RERL      |     Gmin      |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |   R factor    | ext. R factor |    MOS-LQ     |    MOS-CQ     |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |   RX config   |   reserved    |          JB nominal           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |          JB maximum           |          JB abs max           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, Default, PartialEq, Clone)]
pub struct VoIPMetricsReportBlock {
    pub xr_header: XRHeader,
    pub ssrc: u32,
    pub loss_rate: u8,
    pub discard_rate: u8,
    pub burst_density: u8,
    pub gap_density: u8,
    pub burst_duration: u16,
    pub gap_duration: u16,
    pub round_trip_delay: u16,
    pub end_system_delay: u16,
    pub signal_level: u8,
    pub noise_level: u8,
    pub rerl: u8,
    pub gmin: u8,
    pub rfactor: u8,
    pub ext_rfactor: u8,
    pub moslq: u8,
    pub moscq: u8,
    pub rxconfig: u8,
    pub reserved: u8,
    pub jbnominal: u16,
    pub jbmaximum: u16,
    pub jbabs_max: u16,
}

impl fmt::Display for VoIPMetricsReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ReportBlock for VoIPMetricsReportBlock {
    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.ssrc]
    }

    fn setup_block_header(&mut self) {
        self.xr_header.block_type = ReportBlockType::VoIPMetrics;
        self.xr_header.type_specific = 0;
        self.xr_header.block_length = (self.raw_size() / 4 - 1) as u16;
    }

    fn unpack_block_header(&mut self) {}

    fn raw_size(&self) -> usize {
        4 + 4 + 4 + 2 * 4 + 10 + 2 * 3
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn ReportBlock + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<VoIPMetricsReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn ReportBlock + Send + Sync> {
        Box::new(self.clone())
    }
}

/// UnknownReportBlock is used to store bytes for any report block
/// that has an unknown Report Block Type.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct UnknownReportBlock {
    pub xr_header: XRHeader,
    pub bytes: Vec<u8>,
}

impl fmt::Display for UnknownReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ReportBlock for UnknownReportBlock {
    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn setup_block_header(&mut self) {
        self.xr_header.block_length = (self.raw_size() / 4 - 1) as u16;
    }

    fn unpack_block_header(&mut self) {}

    fn raw_size(&self) -> usize {
        4 + self.bytes.len()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn ReportBlock + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<UnknownReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn ReportBlock + Send + Sync> {
        Box::new(self.clone())
    }
}

/// The ExtendedReport packet is an Implementation of RTCP Extended
/// reports defined in RFC 3611. It is used to convey detailed
/// information about an RTP stream. Each packet contains one or
/// more report blocks, each of which conveys a different kind of
/// information.
///
///  0                   1                   2                   3
///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |V=2|P|reserved |   PT=XR=207   |             length            |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                              ssrc                             |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// :                         report blocks                         :
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, PartialEq, Default, Clone)]
pub struct ExtendedReport {
    pub sender_ssrc: u32,
    pub reports: Vec<Box<dyn ReportBlock + Send + Sync>>,
}

impl fmt::Display for ExtendedReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/*
impl Packet for ExtendedReport {
    /// Header returns the Header associated with this packet.
    fn header(&self) -> Header {
        Header {
            padding: get_padding_size(self.raw_size()) != 0,
            count: self.sources.len() as u8,
            packet_type: PacketType::ExtendedReport,
            length: ((self.marshal_size() / 4) - 1) as u16,
        }
    }

    /// destination_ssrc returns an array of ssrc values that this packet refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        let mut ssrc = vec![];
        for p in &self.reports {
            ssrc.extend(p.destination_ssrc());
        }
        ssrc
    }

    fn raw_size(&self) -> usize {
        let mut reports_length = 0;
        for p in &self.reports {
            reports_length += p.raw_size();
        }
        HEADER_LENGTH + SSRC_LENGTH + reports_length
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<ExtendedReport>()
            .map_or(false, |a| self == a)
    }

    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for ExtendedReport {
    fn marshal_size(&self) -> usize {
        let l = self.raw_size();
        // align to 32-bit boundary
        l + get_padding_size(l)
    }
}

impl Marshal for ExtendedReport {
    /// marshal_to encodes the ExtendedReport in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        for _, p := range x.reports {
            p.setup_block_header()
        }

        length := wireSize(x)

        // RTCP Header
        header := Header{
            Type:   TypeExtendedReport,
            Length: uint16(length / 4),
        }
        headerBuffer, err := header.Marshal()
        if err != nil {
            return []byte{}, err
        }
        length += len(headerBuffer)

        rawPacket := make([]byte, length)
        buffer := packetBuffer{bytes: rawPacket}

        err = buffer.write(headerBuffer)
        if err != nil {
            return []byte{}, err
        }
        err = buffer.write(x)
        if err != nil {
            return []byte{}, err
        }

        return rawPacket, nil
    }
}

impl Unmarshal for ExtendedReport {
    /// Unmarshal decodes the ExtendedReport from binary
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        var header Header
        if err := header.Unmarshal(b); err != nil {
            return err
        }
        if header.Type != TypeExtendedReport {
            return errWrongType
        }

        buffer := packetBuffer{bytes: b[headerLength:]}
        err := buffer.read(&x.sender_ssrc)
        if err != nil {
            return err
        }

        for len(buffer.bytes) > 0 {
            var block ReportBlock

            headerBuffer := buffer
            xrHeader := XRHeader{}
            err = headerBuffer.read(&xrHeader)
            if err != nil {
                return err
            }

            switch xrHeader.block_type {
            case LossRLEReportBlockType:
                block = new(LossRLEReportBlock)
            case DuplicateRLEReportBlockType:
                block = new(DuplicateRLEReportBlock)
            case PacketReceiptTimesReportBlockType:
                block = new(PacketReceiptTimesReportBlock)
            case ReceiverReferenceTimeReportBlockType:
                block = new(ReceiverReferenceTimeReportBlock)
            case DLRRReportBlockType:
                block = new(DLRRReportBlock)
            case StatisticsSummaryReportBlockType:
                block = new(StatisticsSummaryReportBlock)
            case VoIPMetricsReportBlockType:
                block = new(VoIPMetricsReportBlock)
            default:
                block = new(UnknownReportBlock)
            }

            // We need to limit the amount of data available to
            // this block to the actual length of the block
            blockLength := (int(xrHeader.block_length) + 1) * 4
            blockBuffer := buffer.split(blockLength)
            err = blockBuffer.read(block)
            if err != nil {
                return err
            }
            block.unpack_block_header()
            x.reports = append(x.reports, block)
        }

        return nil
    }
}
 */
