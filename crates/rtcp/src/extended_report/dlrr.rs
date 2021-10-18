use super::*;

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
