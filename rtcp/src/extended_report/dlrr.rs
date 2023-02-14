use super::*;

const DLRR_REPORT_LENGTH: u16 = 12;

/// DLRRReport encodes a single report inside a DLRRReportBlock.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct DLRRReport {
    pub ssrc: u32,
    pub last_rr: u32,
    pub dlrr: u32,
}

impl fmt::Display for DLRRReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct DLRRReportBlock {
    pub reports: Vec<DLRRReport>,
}

impl fmt::Display for DLRRReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl DLRRReportBlock {
    pub fn xr_header(&self) -> XRHeader {
        XRHeader {
            block_type: BlockType::DLRR,
            type_specific: 0,
            block_length: (self.raw_size() / 4 - 1) as u16,
        }
    }
}

impl Packet for DLRRReportBlock {
    fn header(&self) -> Header {
        Header::default()
    }

    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        let mut ssrc = Vec::with_capacity(self.reports.len());
        for r in &self.reports {
            ssrc.push(r.ssrc);
        }
        ssrc
    }

    fn raw_size(&self) -> usize {
        XR_HEADER_LENGTH + self.reports.len() * 4 * 3
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<DLRRReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for DLRRReportBlock {
    fn marshal_size(&self) -> usize {
        self.raw_size()
    }
}

impl Marshal for DLRRReportBlock {
    /// marshal_to encodes the DLRRReportBlock in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if buf.remaining_mut() < self.marshal_size() {
            return Err(error::Error::BufferTooShort.into());
        }

        let h = self.xr_header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        for rep in &self.reports {
            buf.put_u32(rep.ssrc);
            buf.put_u32(rep.last_rr);
            buf.put_u32(rep.dlrr);
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for DLRRReportBlock {
    /// Unmarshal decodes the DLRRReportBlock from binary
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
        if block_length % DLRR_REPORT_LENGTH != 0 || raw_packet.remaining() < block_length as usize
        {
            return Err(error::Error::PacketTooShort.into());
        }

        let mut offset = 0;
        let mut reports = vec![];
        while offset < block_length {
            let ssrc = raw_packet.get_u32();
            let last_rr = raw_packet.get_u32();
            let dlrr = raw_packet.get_u32();
            reports.push(DLRRReport {
                ssrc,
                last_rr,
                dlrr,
            });
            offset += DLRR_REPORT_LENGTH;
        }

        Ok(DLRRReportBlock { reports })
    }
}
