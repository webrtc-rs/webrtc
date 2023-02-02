use super::*;

const SSR_REPORT_BLOCK_LENGTH: u16 = 4 + 2 * 2 + 4 * 6 + 4;

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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct StatisticsSummaryReportBlock {
    //not included in marshal/unmarshal
    pub loss_reports: bool,
    pub duplicate_reports: bool,
    pub jitter_reports: bool,
    pub ttl_or_hop_limit: TTLorHopLimitType,

    //marshal/unmarshal
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
        write!(f, "{self:?}")
    }
}

/// TTLorHopLimitType encodes values for the ToH field in
/// a StatisticsSummaryReportBlock
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
        write!(f, "{s}")
    }
}

impl StatisticsSummaryReportBlock {
    pub fn xr_header(&self) -> XRHeader {
        let mut type_specific = 0x00;
        if self.loss_reports {
            type_specific |= 0x80;
        }
        if self.duplicate_reports {
            type_specific |= 0x40;
        }
        if self.jitter_reports {
            type_specific |= 0x20;
        }
        type_specific |= (self.ttl_or_hop_limit as u8 & 0x03) << 3;

        XRHeader {
            block_type: BlockType::StatisticsSummary,
            type_specific,
            block_length: (self.raw_size() / 4 - 1) as u16,
        }
    }
}

impl Packet for StatisticsSummaryReportBlock {
    fn header(&self) -> Header {
        Header::default()
    }

    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.ssrc]
    }

    fn raw_size(&self) -> usize {
        XR_HEADER_LENGTH + SSR_REPORT_BLOCK_LENGTH as usize
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<StatisticsSummaryReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for StatisticsSummaryReportBlock {
    fn marshal_size(&self) -> usize {
        self.raw_size()
    }
}

impl Marshal for StatisticsSummaryReportBlock {
    /// marshal_to encodes the StatisticsSummaryReportBlock in binary
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
        buf.put_u32(self.lost_packets);
        buf.put_u32(self.dup_packets);
        buf.put_u32(self.min_jitter);
        buf.put_u32(self.max_jitter);
        buf.put_u32(self.mean_jitter);
        buf.put_u32(self.dev_jitter);
        buf.put_u8(self.min_ttl_or_hl);
        buf.put_u8(self.max_ttl_or_hl);
        buf.put_u8(self.mean_ttl_or_hl);
        buf.put_u8(self.dev_ttl_or_hl);

        Ok(self.marshal_size())
    }
}

impl Unmarshal for StatisticsSummaryReportBlock {
    /// Unmarshal decodes the StatisticsSummaryReportBlock from binary
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
        if block_length != SSR_REPORT_BLOCK_LENGTH || raw_packet.remaining() < block_length as usize
        {
            return Err(error::Error::PacketTooShort.into());
        }

        let loss_reports = xr_header.type_specific & 0x80 != 0;
        let duplicate_reports = xr_header.type_specific & 0x40 != 0;
        let jitter_reports = xr_header.type_specific & 0x20 != 0;
        let ttl_or_hop_limit: TTLorHopLimitType = ((xr_header.type_specific & 0x18) >> 3).into();

        let ssrc = raw_packet.get_u32();
        let begin_seq = raw_packet.get_u16();
        let end_seq = raw_packet.get_u16();
        let lost_packets = raw_packet.get_u32();
        let dup_packets = raw_packet.get_u32();
        let min_jitter = raw_packet.get_u32();
        let max_jitter = raw_packet.get_u32();
        let mean_jitter = raw_packet.get_u32();
        let dev_jitter = raw_packet.get_u32();
        let min_ttl_or_hl = raw_packet.get_u8();
        let max_ttl_or_hl = raw_packet.get_u8();
        let mean_ttl_or_hl = raw_packet.get_u8();
        let dev_ttl_or_hl = raw_packet.get_u8();

        Ok(StatisticsSummaryReportBlock {
            loss_reports,
            duplicate_reports,
            jitter_reports,
            ttl_or_hop_limit,

            ssrc,
            begin_seq,
            end_seq,
            lost_packets,
            dup_packets,
            min_jitter,
            max_jitter,
            mean_jitter,
            dev_jitter,
            min_ttl_or_hl,
            max_ttl_or_hl,
            mean_ttl_or_hl,
            dev_ttl_or_hl,
        })
    }
}
