use super::*;

const PRT_REPORT_BLOCK_MIN_LENGTH: u16 = 8;

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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct PacketReceiptTimesReportBlock {
    //not included in marshal/unmarshal
    pub t: u8,

    //marshal/unmarshal
    pub ssrc: u32,
    pub begin_seq: u16,
    pub end_seq: u16,
    pub receipt_time: Vec<u32>,
}

impl fmt::Display for PacketReceiptTimesReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl PacketReceiptTimesReportBlock {
    pub fn xr_header(&self) -> XRHeader {
        XRHeader {
            block_type: BlockType::PacketReceiptTimes,
            type_specific: self.t & 0x0F,
            block_length: (self.raw_size() / 4 - 1) as u16,
        }
    }
}

impl Packet for PacketReceiptTimesReportBlock {
    fn header(&self) -> Header {
        Header::default()
    }

    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.ssrc]
    }

    fn raw_size(&self) -> usize {
        XR_HEADER_LENGTH + PRT_REPORT_BLOCK_MIN_LENGTH as usize + self.receipt_time.len() * 4
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<PacketReceiptTimesReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for PacketReceiptTimesReportBlock {
    fn marshal_size(&self) -> usize {
        self.raw_size()
    }
}

impl Marshal for PacketReceiptTimesReportBlock {
    /// marshal_to encodes the PacketReceiptTimesReportBlock in binary
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
        for rt in &self.receipt_time {
            buf.put_u32(*rt);
        }

        Ok(self.marshal_size())
    }
}

impl Unmarshal for PacketReceiptTimesReportBlock {
    /// Unmarshal decodes the PacketReceiptTimesReportBlock from binary
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
        if block_length < PRT_REPORT_BLOCK_MIN_LENGTH
            || (block_length - PRT_REPORT_BLOCK_MIN_LENGTH) % 4 != 0
            || raw_packet.remaining() < block_length as usize
        {
            return Err(error::Error::PacketTooShort.into());
        }

        let t = xr_header.type_specific & 0x0F;

        let ssrc = raw_packet.get_u32();
        let begin_seq = raw_packet.get_u16();
        let end_seq = raw_packet.get_u16();

        let remaining = block_length - PRT_REPORT_BLOCK_MIN_LENGTH;
        let mut receipt_time = vec![];
        for _ in 0..remaining / 4 {
            receipt_time.push(raw_packet.get_u32());
        }

        Ok(PacketReceiptTimesReportBlock {
            t,

            ssrc,
            begin_seq,
            end_seq,
            receipt_time,
        })
    }
}
