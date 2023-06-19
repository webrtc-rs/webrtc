#[cfg(test)]
mod extended_report_test;

pub mod dlrr;
pub mod prt;
pub mod rle;
pub mod rrt;
pub mod ssr;
pub mod unknown;
pub mod vm;

use std::any::Any;
use std::fmt;

use bytes::{Buf, BufMut, Bytes};
pub use dlrr::{DLRRReport, DLRRReportBlock};
pub use prt::PacketReceiptTimesReportBlock;
pub use rle::{Chunk, ChunkType, DuplicateRLEReportBlock, LossRLEReportBlock, RLEReportBlock};
pub use rrt::ReceiverReferenceTimeReportBlock;
pub use ssr::{StatisticsSummaryReportBlock, TTLorHopLimitType};
pub use unknown::UnknownReportBlock;
use util::marshal::{Marshal, MarshalSize, Unmarshal};
pub use vm::VoIPMetricsReportBlock;

use crate::error;
use crate::header::{Header, PacketType, HEADER_LENGTH, SSRC_LENGTH};
use crate::packet::Packet;
use crate::util::{get_padding_size, put_padding};

type Result<T> = std::result::Result<T, util::Error>;

const XR_HEADER_LENGTH: usize = 4;

/// BlockType specifies the type of report in a report block
/// Extended Report block types from RFC 3611.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum BlockType {
    #[default]
    Unknown = 0,
    LossRLE = 1,               // RFC 3611, section 4.1
    DuplicateRLE = 2,          // RFC 3611, section 4.2
    PacketReceiptTimes = 3,    // RFC 3611, section 4.3
    ReceiverReferenceTime = 4, // RFC 3611, section 4.4
    DLRR = 5,                  // RFC 3611, section 4.5
    StatisticsSummary = 6,     // RFC 3611, section 4.6
    VoIPMetrics = 7,           // RFC 3611, section 4.7
}

impl From<u8> for BlockType {
    fn from(v: u8) -> Self {
        match v {
            1 => BlockType::LossRLE,
            2 => BlockType::DuplicateRLE,
            3 => BlockType::PacketReceiptTimes,
            4 => BlockType::ReceiverReferenceTime,
            5 => BlockType::DLRR,
            6 => BlockType::StatisticsSummary,
            7 => BlockType::VoIPMetrics,
            _ => BlockType::Unknown,
        }
    }
}

/// converts the Extended report block types into readable strings
impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            BlockType::LossRLE => "LossRLEReportBlockType",
            BlockType::DuplicateRLE => "DuplicateRLEReportBlockType",
            BlockType::PacketReceiptTimes => "PacketReceiptTimesReportBlockType",
            BlockType::ReceiverReferenceTime => "ReceiverReferenceTimeReportBlockType",
            BlockType::DLRR => "DLRRReportBlockType",
            BlockType::StatisticsSummary => "StatisticsSummaryReportBlockType",
            BlockType::VoIPMetrics => "VoIPMetricsReportBlockType",
            _ => "UnknownReportBlockType",
        };
        write!(f, "{s}")
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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct XRHeader {
    pub block_type: BlockType,
    pub type_specific: TypeSpecificField,
    pub block_length: u16,
}

impl MarshalSize for XRHeader {
    fn marshal_size(&self) -> usize {
        XR_HEADER_LENGTH
    }
}

impl Marshal for XRHeader {
    /// marshal_to encodes the ExtendedReport in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if buf.remaining_mut() < XR_HEADER_LENGTH {
            return Err(error::Error::BufferTooShort.into());
        }

        buf.put_u8(self.block_type as u8);
        buf.put_u8(self.type_specific);
        buf.put_u16(self.block_length);

        Ok(XR_HEADER_LENGTH)
    }
}

impl Unmarshal for XRHeader {
    /// Unmarshal decodes the ExtendedReport from binary
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        if raw_packet.remaining() < XR_HEADER_LENGTH {
            return Err(error::Error::PacketTooShort.into());
        }

        let block_type: BlockType = raw_packet.get_u8().into();
        let type_specific = raw_packet.get_u8();
        let block_length = raw_packet.get_u16();

        Ok(XRHeader {
            block_type,
            type_specific,
            block_length,
        })
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
    pub reports: Vec<Box<dyn Packet + Send + Sync>>,
}

impl fmt::Display for ExtendedReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Packet for ExtendedReport {
    /// Header returns the Header associated with this packet.
    fn header(&self) -> Header {
        Header {
            padding: get_padding_size(self.raw_size()) != 0,
            count: 0,
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
        let mut reps_length = 0;
        for rep in &self.reports {
            reps_length += rep.marshal_size();
        }
        HEADER_LENGTH + SSRC_LENGTH + reps_length
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
        if buf.remaining_mut() < self.marshal_size() {
            return Err(error::Error::BufferTooShort.into());
        }

        let h = self.header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put_u32(self.sender_ssrc);

        for report in &self.reports {
            let n = report.marshal_to(buf)?;
            buf = &mut buf[n..];
        }

        if h.padding {
            put_padding(buf, self.raw_size());
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for ExtendedReport {
    /// Unmarshal decodes the ExtendedReport from binary
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        let raw_packet_len = raw_packet.remaining();
        if raw_packet_len < (HEADER_LENGTH + SSRC_LENGTH) {
            return Err(error::Error::PacketTooShort.into());
        }

        let header = Header::unmarshal(raw_packet)?;
        if header.packet_type != PacketType::ExtendedReport {
            return Err(error::Error::WrongType.into());
        }

        let sender_ssrc = raw_packet.get_u32();

        let mut offset = HEADER_LENGTH + SSRC_LENGTH;
        let mut reports = vec![];
        while raw_packet.remaining() > 0 {
            if offset + XR_HEADER_LENGTH > raw_packet_len {
                return Err(error::Error::PacketTooShort.into());
            }

            let block_type: BlockType = raw_packet.chunk()[0].into();
            let report: Box<dyn Packet + Send + Sync> = match block_type {
                BlockType::LossRLE => Box::new(LossRLEReportBlock::unmarshal(raw_packet)?),
                BlockType::DuplicateRLE => {
                    Box::new(DuplicateRLEReportBlock::unmarshal(raw_packet)?)
                }
                BlockType::PacketReceiptTimes => {
                    Box::new(PacketReceiptTimesReportBlock::unmarshal(raw_packet)?)
                }
                BlockType::ReceiverReferenceTime => {
                    Box::new(ReceiverReferenceTimeReportBlock::unmarshal(raw_packet)?)
                }
                BlockType::DLRR => Box::new(DLRRReportBlock::unmarshal(raw_packet)?),
                BlockType::StatisticsSummary => {
                    Box::new(StatisticsSummaryReportBlock::unmarshal(raw_packet)?)
                }
                BlockType::VoIPMetrics => Box::new(VoIPMetricsReportBlock::unmarshal(raw_packet)?),
                _ => Box::new(UnknownReportBlock::unmarshal(raw_packet)?),
            };

            offset += report.marshal_size();
            reports.push(report);
        }

        Ok(ExtendedReport {
            sender_ssrc,
            reports,
        })
    }
}
