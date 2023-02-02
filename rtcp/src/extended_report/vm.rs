use super::*;

const VM_REPORT_BLOCK_LENGTH: u16 = 4 + 4 + 2 * 4 + 10 + 2 * 3;

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
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct VoIPMetricsReportBlock {
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
    pub mos_lq: u8,
    pub mos_cq: u8,
    pub rx_config: u8,
    pub reserved: u8,
    pub jb_nominal: u16,
    pub jb_maximum: u16,
    pub jb_abs_max: u16,
}

impl fmt::Display for VoIPMetricsReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl VoIPMetricsReportBlock {
    pub fn xr_header(&self) -> XRHeader {
        XRHeader {
            block_type: BlockType::VoIPMetrics,
            type_specific: 0,
            block_length: (self.raw_size() / 4 - 1) as u16,
        }
    }
}

impl Packet for VoIPMetricsReportBlock {
    fn header(&self) -> Header {
        Header::default()
    }

    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![self.ssrc]
    }

    fn raw_size(&self) -> usize {
        XR_HEADER_LENGTH + VM_REPORT_BLOCK_LENGTH as usize
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<VoIPMetricsReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for VoIPMetricsReportBlock {
    fn marshal_size(&self) -> usize {
        self.raw_size()
    }
}

impl Marshal for VoIPMetricsReportBlock {
    /// marshal_to encodes the VoIPMetricsReportBlock in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if buf.remaining_mut() < self.marshal_size() {
            return Err(error::Error::BufferTooShort.into());
        }

        let h = self.xr_header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put_u32(self.ssrc);
        buf.put_u8(self.loss_rate);
        buf.put_u8(self.discard_rate);
        buf.put_u8(self.burst_density);
        buf.put_u8(self.gap_density);
        buf.put_u16(self.burst_duration);
        buf.put_u16(self.gap_duration);
        buf.put_u16(self.round_trip_delay);
        buf.put_u16(self.end_system_delay);
        buf.put_u8(self.signal_level);
        buf.put_u8(self.noise_level);
        buf.put_u8(self.rerl);
        buf.put_u8(self.gmin);
        buf.put_u8(self.rfactor);
        buf.put_u8(self.ext_rfactor);
        buf.put_u8(self.mos_lq);
        buf.put_u8(self.mos_cq);
        buf.put_u8(self.rx_config);
        buf.put_u8(self.reserved);
        buf.put_u16(self.jb_nominal);
        buf.put_u16(self.jb_maximum);
        buf.put_u16(self.jb_abs_max);

        Ok(self.marshal_size())
    }
}

impl Unmarshal for VoIPMetricsReportBlock {
    /// Unmarshal decodes the VoIPMetricsReportBlock from binary
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
        if block_length != VM_REPORT_BLOCK_LENGTH || raw_packet.remaining() < block_length as usize
        {
            return Err(error::Error::PacketTooShort.into());
        }

        let ssrc = raw_packet.get_u32();
        let loss_rate = raw_packet.get_u8();
        let discard_rate = raw_packet.get_u8();
        let burst_density = raw_packet.get_u8();
        let gap_density = raw_packet.get_u8();
        let burst_duration = raw_packet.get_u16();
        let gap_duration = raw_packet.get_u16();
        let round_trip_delay = raw_packet.get_u16();
        let end_system_delay = raw_packet.get_u16();
        let signal_level = raw_packet.get_u8();
        let noise_level = raw_packet.get_u8();
        let rerl = raw_packet.get_u8();
        let gmin = raw_packet.get_u8();
        let rfactor = raw_packet.get_u8();
        let ext_rfactor = raw_packet.get_u8();
        let mos_lq = raw_packet.get_u8();
        let mos_cq = raw_packet.get_u8();
        let rx_config = raw_packet.get_u8();
        let reserved = raw_packet.get_u8();
        let jb_nominal = raw_packet.get_u16();
        let jb_maximum = raw_packet.get_u16();
        let jb_abs_max = raw_packet.get_u16();

        Ok(VoIPMetricsReportBlock {
            ssrc,
            loss_rate,
            discard_rate,
            burst_density,
            gap_density,
            burst_duration,
            gap_duration,
            round_trip_delay,
            end_system_delay,
            signal_level,
            noise_level,
            rerl,
            gmin,
            rfactor,
            ext_rfactor,
            mos_lq,
            mos_cq,
            rx_config,
            reserved,
            jb_nominal,
            jb_maximum,
            jb_abs_max,
        })
    }
}
