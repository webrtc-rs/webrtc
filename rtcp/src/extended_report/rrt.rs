use super::*;

const RRT_REPORT_BLOCK_LENGTH: u16 = 8;

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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct ReceiverReferenceTimeReportBlock {
    pub ntp_timestamp: u64,
}

impl fmt::Display for ReceiverReferenceTimeReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl ReceiverReferenceTimeReportBlock {
    pub fn xr_header(&self) -> XRHeader {
        XRHeader {
            block_type: BlockType::ReceiverReferenceTime,
            type_specific: 0,
            block_length: (self.raw_size() / 4 - 1) as u16,
        }
    }
}

impl Packet for ReceiverReferenceTimeReportBlock {
    fn header(&self) -> Header {
        Header::default()
    }

    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn raw_size(&self) -> usize {
        XR_HEADER_LENGTH + RRT_REPORT_BLOCK_LENGTH as usize
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<ReceiverReferenceTimeReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for ReceiverReferenceTimeReportBlock {
    fn marshal_size(&self) -> usize {
        self.raw_size()
    }
}

impl Marshal for ReceiverReferenceTimeReportBlock {
    /// marshal_to encodes the ReceiverReferenceTimeReportBlock in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if buf.remaining_mut() < self.marshal_size() {
            return Err(error::Error::BufferTooShort.into());
        }

        let h = self.xr_header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put_u64(self.ntp_timestamp);

        Ok(self.marshal_size())
    }
}

impl Unmarshal for ReceiverReferenceTimeReportBlock {
    /// Unmarshal decodes the ReceiverReferenceTimeReportBlock from binary
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
        if block_length != RRT_REPORT_BLOCK_LENGTH || raw_packet.remaining() < block_length as usize
        {
            return Err(error::Error::PacketTooShort.into());
        }

        let ntp_timestamp = raw_packet.get_u64();

        Ok(ReceiverReferenceTimeReportBlock { ntp_timestamp })
    }
}
