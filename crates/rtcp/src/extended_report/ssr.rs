use super::*;

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
