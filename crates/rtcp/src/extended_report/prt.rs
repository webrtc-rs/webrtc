use super::*;

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
