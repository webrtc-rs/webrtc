use super::*;

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
