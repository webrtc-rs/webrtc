use bytes::{BufMut, Bytes, BytesMut};

use super::h264::ANNEXB_NALUSTART_CODE;
use crate::error::{Error, Result};
use crate::packetizer::{Depacketizer, Payloader};

#[cfg(test)]
mod h265_test;

pub static ANNEXB_3_NALUSTART_CODE: Bytes = Bytes::from_static(&[0x00, 0x00, 0x01]);
pub static SING_PAYLOAD_HDR: Bytes = Bytes::from_static(&[0x1C, 0x01]);
pub static AGGR_PAYLOAD_HDR: Bytes = Bytes::from_static(&[0x60, 0x01]);
pub static FRAG_PAYLOAD_HDR: Bytes = Bytes::from_static(&[0x62, 0x01]);
pub static FU_HDR_IDR_S: u8 = 0x93;
pub static FU_HDR_IDR_M: u8 = 0x13;
pub static FU_HDR_IDR_E: u8 = 0x53;
pub static FU_HDR_P_S: u8 = 0x81;
pub static FU_HDR_P_M: u8 = 0x01;
pub static FU_HDR_P_E: u8 = 0x41;
pub static FU_HDR_B_S: u8 = 0x80;
pub static FU_HDR_B_M: u8 = 0x00;
pub static FU_HDR_B_E: u8 = 0x40;
pub const RTP_OUTBOUND_MTU: usize = 1200;
pub const H265FRAGMENTATION_UNIT_HEADER_SIZE: usize = 1;
pub const NAL_HEADER_SIZE: usize = 2;

#[derive(PartialEq, Hash, Debug, Copy, Clone)]
pub enum UnitType {
    VPS = 32,
    SPS = 33,
    PPS = 34,
    CRA = 21,
    SEI = 39,
    IDR = 19,
    PFR = 1,
    BFR = 0,
    IGNORE = -1,
}
impl UnitType {
    pub fn for_id(id: u8) -> Result<UnitType> {
        if id > 64 {
            Err(Error::ErrUnhandledNaluType)
        } else {
            let t = match id {
                32 => UnitType::VPS,
                33 => UnitType::SPS,
                34 => UnitType::PPS,
                21 => UnitType::CRA,
                39 => UnitType::SEI,
                19 => UnitType::IDR,
                1 => UnitType::PFR,
                0 => UnitType::BFR,
                _ => UnitType::IGNORE, // shouldn't happen
            };
            Ok(t)
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct HevcPayloader {
    vps_nalu: Option<Bytes>,
    sps_nalu: Option<Bytes>,
    pps_nalu: Option<Bytes>,
}

impl HevcPayloader {
    pub fn parse(nalu: &Bytes) -> (Vec<usize>, usize) {
        let finder = memchr::memmem::Finder::new(&ANNEXB_NALUSTART_CODE);
        let nals = finder.find_iter(nalu).collect::<Vec<usize>>();
        if nals.is_empty() {
            let finder = memchr::memmem::Finder::new(&ANNEXB_3_NALUSTART_CODE);
            return (finder.find_iter(nalu).collect::<Vec<usize>>(), 3);
        }
        (nals, 4)
    }

    fn emit(&mut self, nalu: &Bytes, mtu: usize, payloads: &mut Vec<Bytes>) {
        if nalu.is_empty() {
            return;
        }
        let payload_header = H265NALUHeader::new(nalu[0], nalu[1]);
        let payload_nalu_type = payload_header.nalu_type();
        let nalu_type = UnitType::for_id(payload_nalu_type).unwrap_or(UnitType::IGNORE);
        if nalu_type == UnitType::IGNORE {
            return;
        } else if nalu_type == UnitType::VPS {
            self.vps_nalu.replace(nalu.clone());
        } else if nalu_type == UnitType::SPS {
            self.sps_nalu.replace(nalu.clone());
        } else if nalu_type == UnitType::PPS {
            self.pps_nalu.replace(nalu.clone());
        }
        if let (Some(vps_nalu), Some(sps_nalu), Some(pps_nalu)) =
            (&self.vps_nalu, &self.sps_nalu, &self.pps_nalu)
        {
            // Pack current NALU with SPS and PPS as STAP-A
            let vps_len = (vps_nalu.len() as u16).to_be_bytes();
            let sps_len = (sps_nalu.len() as u16).to_be_bytes();
            let pps_len = (pps_nalu.len() as u16).to_be_bytes();

            // TODO DONL not impl yet
            let mut aggr_nalu = BytesMut::new();
            aggr_nalu.extend_from_slice(&AGGR_PAYLOAD_HDR);
            aggr_nalu.extend_from_slice(&vps_len);
            aggr_nalu.extend_from_slice(vps_nalu);
            aggr_nalu.extend_from_slice(&sps_len);
            aggr_nalu.extend_from_slice(sps_nalu);
            aggr_nalu.extend_from_slice(&pps_len);
            aggr_nalu.extend_from_slice(pps_nalu);
            if aggr_nalu.len() <= mtu {
                payloads.push(Bytes::from(aggr_nalu));
                self.vps_nalu.take();
                self.sps_nalu.take();
                self.pps_nalu.take();
                return;
            }
        } else if nalu_type == UnitType::VPS
            || nalu_type == UnitType::SPS
            || nalu_type == UnitType::PPS
        {
            return;
        }
        // if self.sps_nalu.is_some() && self.pps_nalu.is_some() {
        //     self.sps_nalu = None;
        //     self.pps_nalu = None;
        // }

        // Single NALU
        if nalu.len() <= mtu {
            payloads.push(nalu.clone());
            return;
        }
        let max_fragment_size =
            mtu as isize - NAL_HEADER_SIZE as isize - H265FRAGMENTATION_UNIT_HEADER_SIZE as isize;
        let nalu_data = nalu;
        let mut nalu_data_index = 2;
        let nalu_data_length = nalu.len() as isize - nalu_data_index;
        let mut nalu_data_remaining = nalu_data_length;
        if std::cmp::min(max_fragment_size, nalu_data_remaining) <= 0 {
            return;
        }
        while nalu_data_remaining > 0 {
            let current_fragment_size = std::cmp::min(max_fragment_size, nalu_data_remaining);
            //out: = make([]byte, fuaHeaderSize + currentFragmentSize)
            let mut out = BytesMut::with_capacity(
                H265FRAGMENTATION_UNIT_HEADER_SIZE + current_fragment_size as usize,
            );
            out.extend_from_slice(&FRAG_PAYLOAD_HDR);
            let is_first = nalu_data_index == 2;
            let is_last = !is_first && current_fragment_size < max_fragment_size;
            /*
            +---------------+
            |0|1|2|3|4|5|6|7|
            +-+-+-+-+-+-+-+-+
            |S|E|  fu_type  |
            +---------------+
            */
            if nalu_type == UnitType::IDR {
                if is_first {
                    out.put_u8(FU_HDR_IDR_S);
                } else if is_last {
                    out.put_u8(FU_HDR_IDR_E);
                } else {
                    out.put_u8(FU_HDR_IDR_M);
                }
            } else if nalu_type == UnitType::PFR {
                if is_first {
                    out.put_u8(FU_HDR_P_S);
                } else if is_last {
                    out.put_u8(FU_HDR_P_E);
                } else {
                    out.put_u8(FU_HDR_P_M);
                }
            } else if nalu_type == UnitType::BFR {
                if is_first {
                    out.put_u8(FU_HDR_B_S);
                } else if is_last {
                    out.put_u8(FU_HDR_B_E);
                } else {
                    out.put_u8(FU_HDR_B_M);
                }
            }

            out.extend_from_slice(
                &nalu_data
                    [nalu_data_index as usize..(nalu_data_index + current_fragment_size) as usize],
            );
            // println!("pkt payload {:?}", &out[0..5]);
            payloads.push(out.freeze());

            nalu_data_remaining -= current_fragment_size;
            nalu_data_index += current_fragment_size;
        }
    }
}

impl Payloader for HevcPayloader {
    /// Payload fragments a H264 packet across one or more byte arrays
    fn payload(&mut self, mtu: usize, payload: &Bytes) -> Result<Vec<Bytes>> {
        if payload.is_empty() || mtu == 0 {
            return Ok(vec![]);
        }

        let mut payloads = vec![];

        let (nal_idxs, offset) = HevcPayloader::parse(payload);
        let nal_len = nal_idxs.len();
        for (i, start) in nal_idxs.iter().enumerate() {
            let end = if (i + 1) < nal_len {
                nal_idxs[i + 1]
            } else {
                payload.len()
            };
            // println!(
            //     "start {}, end {} payload {:?}",
            //     start,
            //     end,
            //     &payload
            //         .slice((start + offset)..(start + offset + 5))
            //         .to_vec()
            // );
            self.emit(&payload.slice((start + offset)..end), mtu, &mut payloads);
        }

        Ok(payloads)
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(self.clone())
    }
}

///
/// Network Abstraction Unit Header implementation
///

const H265NALU_HEADER_SIZE: usize = 2;
/// <https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.2>
const H265NALU_AGGREGATION_PACKET_TYPE: u8 = 48;
/// <https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.3>
const H265NALU_FRAGMENTATION_UNIT_TYPE: u8 = 49;
/// <https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.4>
const H265NALU_PACI_PACKET_TYPE: u8 = 50;

/// H265NALUHeader is a H265 NAL Unit Header
///
/// ```text
/// +---------------+---------------+
/// |0|1|2|3|4|5|6|7|0|1|2|3|4|5|6|7|
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |F|   Type    |  layer_id  | tid|
/// +-------------+-----------------+
/// ```
///
/// ## Specifications
///
/// * [RFC 7798 §1.1.4]
///
/// [RFC 7798 §1.1.4]: https://tools.ietf.org/html/rfc7798#section-1.1.4
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct H265NALUHeader(pub u16);

impl H265NALUHeader {
    pub fn new(high_byte: u8, low_byte: u8) -> Self {
        H265NALUHeader(((high_byte as u16) << 8) | low_byte as u16)
    }

    /// f is the forbidden bit, should always be 0.
    pub fn f(&self) -> bool {
        (self.0 >> 15) != 0
    }

    /// nalu_type of NAL Unit.
    pub fn nalu_type(&self) -> u8 {
        // 01111110 00000000
        const MASK: u16 = 0b01111110 << 8;
        ((self.0 & MASK) >> (8 + 1)) as u8
    }

    /// is_type_vcl_unit returns whether or not the NAL Unit type is a VCL NAL unit.
    pub fn is_type_vcl_unit(&self) -> bool {
        // Type is coded on 6 bits
        const MSB_MASK: u8 = 0b00100000;
        (self.nalu_type() & MSB_MASK) == 0
    }

    /// layer_id should always be 0 in non-3D HEVC context.
    pub fn layer_id(&self) -> u8 {
        // 00000001 11111000
        const MASK: u16 = (0b00000001 << 8) | 0b11111000;
        ((self.0 & MASK) >> 3) as u8
    }

    /// tid is the temporal identifier of the NAL unit +1.
    pub fn tid(&self) -> u8 {
        const MASK: u16 = 0b00000111;
        (self.0 & MASK) as u8
    }

    /// is_aggregation_packet returns whether or not the packet is an Aggregation packet.
    pub fn is_aggregation_packet(&self) -> bool {
        self.nalu_type() == H265NALU_AGGREGATION_PACKET_TYPE
    }

    /// is_fragmentation_unit returns whether or not the packet is a Fragmentation Unit packet.
    pub fn is_fragmentation_unit(&self) -> bool {
        self.nalu_type() == H265NALU_FRAGMENTATION_UNIT_TYPE
    }

    /// is_paci_packet returns whether or not the packet is a PACI packet.
    pub fn is_paci_packet(&self) -> bool {
        self.nalu_type() == H265NALU_PACI_PACKET_TYPE
    }
}

///
/// Single NAL Unit Packet implementation
///
/// H265SingleNALUnitPacket represents a NALU packet, containing exactly one NAL unit.
///     0                   1                   2                   3
///    0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |           PayloadHdr          |      DONL (conditional)       |
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |                                                               |
///   |                  NAL unit payload data                        |
///   |                                                               |
///   |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |                               :...OPTIONAL RTP padding        |
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// ## Specifications
///
/// * [RFC 7798 §4.4.1]
///
/// [RFC 7798 §4.4.1]: https://tools.ietf.org/html/rfc7798#section-4.4.1
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct H265SingleNALUnitPacket {
    /// payload_header is the header of the H265 packet.
    payload_header: H265NALUHeader,
    /// donl is a 16-bit field, that may or may not be present.
    donl: Option<u16>,
    /// payload of the fragmentation unit.
    payload: Bytes,

    might_need_donl: bool,
}

impl H265SingleNALUnitPacket {
    /// with_donl can be called to specify whether or not DONL might be parsed.
    /// DONL may need to be parsed if `sprop-max-don-diff` is greater than 0 on the RTP stream.
    pub fn with_donl(&mut self, value: bool) {
        self.might_need_donl = value;
    }

    /// depacketize parses the passed byte slice and stores the result in the H265SingleNALUnitPacket this method is called upon.
    fn depacketize(&mut self, payload: &Bytes) -> Result<()> {
        if payload.len() <= H265NALU_HEADER_SIZE {
            return Err(Error::ErrShortPacket);
        }

        let payload_header = H265NALUHeader::new(payload[0], payload[1]);
        if payload_header.f() {
            return Err(Error::ErrH265CorruptedPacket);
        }
        if payload_header.is_fragmentation_unit()
            || payload_header.is_paci_packet()
            || payload_header.is_aggregation_packet()
        {
            return Err(Error::ErrInvalidH265PacketType);
        }

        let mut payload = payload.slice(2..);

        if self.might_need_donl {
            // sizeof(uint16)
            if payload.len() <= 2 {
                return Err(Error::ErrShortPacket);
            }

            let donl = ((payload[0] as u16) << 8) | (payload[1] as u16);
            self.donl = Some(donl);
            payload = payload.slice(2..);
        }

        self.payload_header = payload_header;
        self.payload = payload;

        Ok(())
    }

    /// payload_header returns the NALU header of the packet.
    pub fn payload_header(&self) -> H265NALUHeader {
        self.payload_header
    }

    /// donl returns the DONL of the packet.
    pub fn donl(&self) -> Option<u16> {
        self.donl
    }

    /// payload returns the Fragmentation Unit packet payload.
    pub fn payload(&self) -> Bytes {
        self.payload.clone()
    }
}

///
/// Aggregation Packets implementation
///
/// H265AggregationUnitFirst represent the First Aggregation Unit in an AP.
///
///    0                   1                   2                   3
///    0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///                   :       DONL (conditional)      |   NALU size   |
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |   NALU size   |                                               |
///   +-+-+-+-+-+-+-+-+         NAL unit                              |
///   |                                                               |
///   |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |                               :
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// ## Specifications
///
/// * [RFC 7798 §4.4.2]
///
/// [RFC 7798 §4.4.2]: https://tools.ietf.org/html/rfc7798#section-4.4.2
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct H265AggregationUnitFirst {
    donl: Option<u16>,
    nal_unit_size: u16,
    nal_unit: Bytes,
}

impl H265AggregationUnitFirst {
    /// donl field, when present, specifies the value of the 16 least
    /// significant bits of the decoding order number of the aggregated NAL
    /// unit.
    pub fn donl(&self) -> Option<u16> {
        self.donl
    }

    /// nalu_size represents the size, in bytes, of the nal_unit.
    pub fn nalu_size(&self) -> u16 {
        self.nal_unit_size
    }

    /// nal_unit payload.
    pub fn nal_unit(&self) -> Bytes {
        self.nal_unit.clone()
    }
}

/// H265AggregationUnit represent the an Aggregation Unit in an AP, which is not the first one.
///
///    0                   1                   2                   3
///    0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///                   : DOND (cond)   |          NALU size            |
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |                                                               |
///   |                       NAL unit                                |
///   |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |                               :
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// ## Specifications
///
/// * [RFC 7798 §4.4.2]
///
/// [RFC 7798 §4.4.2]: https://tools.ietf.org/html/rfc7798#section-4.4.2
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct H265AggregationUnit {
    dond: Option<u8>,
    nal_unit_size: u16,
    nal_unit: Bytes,
}

impl H265AggregationUnit {
    /// dond field plus 1 specifies the difference between
    /// the decoding order number values of the current aggregated NAL unit
    /// and the preceding aggregated NAL unit in the same AP.
    pub fn dond(&self) -> Option<u8> {
        self.dond
    }

    /// nalu_size represents the size, in bytes, of the nal_unit.
    pub fn nalu_size(&self) -> u16 {
        self.nal_unit_size
    }

    /// nal_unit payload.
    pub fn nal_unit(&self) -> Bytes {
        self.nal_unit.clone()
    }
}

/// H265AggregationPacket represents an Aggregation packet.
///   0                   1                   2                   3
///    0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |    PayloadHdr (Type=48)       |                               |
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+                               |
///   |                                                               |
///   |             two or more aggregation units                     |
///   |                                                               |
///   |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///   |                               :...OPTIONAL RTP padding        |
///   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// ## Specifications
///
/// * [RFC 7798 §4.4.2]
///
/// [RFC 7798 §4.4.2]: https://tools.ietf.org/html/rfc7798#section-4.4.2
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct H265AggregationPacket {
    first_unit: Option<H265AggregationUnitFirst>,
    other_units: Vec<H265AggregationUnit>,

    might_need_donl: bool,
}

impl H265AggregationPacket {
    /// with_donl can be called to specify whether or not DONL might be parsed.
    /// DONL may need to be parsed if `sprop-max-don-diff` is greater than 0 on the RTP stream.
    pub fn with_donl(&mut self, value: bool) {
        self.might_need_donl = value;
    }

    /// depacketize parses the passed byte slice and stores the result in the H265AggregationPacket this method is called upon.
    fn depacketize(&mut self, payload: &Bytes) -> Result<()> {
        if payload.len() <= H265NALU_HEADER_SIZE {
            return Err(Error::ErrShortPacket);
        }

        let payload_header = H265NALUHeader::new(payload[0], payload[1]);
        if payload_header.f() {
            return Err(Error::ErrH265CorruptedPacket);
        }
        if !payload_header.is_aggregation_packet() {
            return Err(Error::ErrInvalidH265PacketType);
        }

        // First parse the first aggregation unit
        let mut payload = payload.slice(2..);
        let mut first_unit = H265AggregationUnitFirst::default();

        if self.might_need_donl {
            if payload.len() < 2 {
                return Err(Error::ErrShortPacket);
            }

            let donl = ((payload[0] as u16) << 8) | (payload[1] as u16);
            first_unit.donl = Some(donl);

            payload = payload.slice(2..);
        }
        if payload.len() < 2 {
            return Err(Error::ErrShortPacket);
        }
        first_unit.nal_unit_size = ((payload[0] as u16) << 8) | (payload[1] as u16);
        payload = payload.slice(2..);

        if payload.len() < first_unit.nal_unit_size as usize {
            return Err(Error::ErrShortPacket);
        }

        first_unit.nal_unit = payload.slice(..first_unit.nal_unit_size as usize);
        payload = payload.slice(first_unit.nal_unit_size as usize..);

        // Parse remaining Aggregation Units
        let mut units = vec![]; //H265AggregationUnit
        loop {
            let mut unit = H265AggregationUnit::default();

            if self.might_need_donl {
                if payload.is_empty() {
                    break;
                }

                let dond = payload[0];
                unit.dond = Some(dond);

                payload = payload.slice(1..);
            }

            if payload.len() < 2 {
                break;
            }
            unit.nal_unit_size = ((payload[0] as u16) << 8) | (payload[1] as u16);
            payload = payload.slice(2..);

            if payload.len() < unit.nal_unit_size as usize {
                break;
            }

            unit.nal_unit = payload.slice(..unit.nal_unit_size as usize);
            payload = payload.slice(unit.nal_unit_size as usize..);

            units.push(unit);
        }

        // There need to be **at least** two Aggregation Units (first + another one)
        if units.is_empty() {
            return Err(Error::ErrShortPacket);
        }

        self.first_unit = Some(first_unit);
        self.other_units = units;

        Ok(())
    }

    /// first_unit returns the first Aggregated Unit of the packet.
    pub fn first_unit(&self) -> Option<&H265AggregationUnitFirst> {
        self.first_unit.as_ref()
    }

    /// other_units returns the all the other Aggregated Unit of the packet (excluding the first one).
    pub fn other_units(&self) -> &[H265AggregationUnit] {
        self.other_units.as_slice()
    }
}

///
/// Fragmentation Unit implementation
///

/// H265FragmentationUnitHeader is a H265 FU Header
/// +---------------+
/// |0|1|2|3|4|5|6|7|
/// +-+-+-+-+-+-+-+-+
/// |S|E|  fu_type   |
/// +---------------+
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct H265FragmentationUnitHeader(pub u8);

impl H265FragmentationUnitHeader {
    /// s represents the start of a fragmented NAL unit.
    pub fn s(&self) -> bool {
        const MASK: u8 = 0b10000000;
        ((self.0 & MASK) >> 7) != 0
    }

    /// e represents the end of a fragmented NAL unit.
    pub fn e(&self) -> bool {
        const MASK: u8 = 0b01000000;
        ((self.0 & MASK) >> 6) != 0
    }

    /// fu_type MUST be equal to the field Type of the fragmented NAL unit.
    pub fn fu_type(&self) -> u8 {
        const MASK: u8 = 0b00111111;
        self.0 & MASK
    }
}

/// H265FragmentationUnitPacket represents a single Fragmentation Unit packet.
///
///  0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |    PayloadHdr (Type=49)       |   FU header   | DONL (cond)   |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-|
/// | DONL (cond)   |                                               |
/// |-+-+-+-+-+-+-+-+                                               |
/// |                         FU payload                            |
/// |                                                               |
/// |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                               :...OPTIONAL RTP padding        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// ## Specifications
///
/// * [RFC 7798 §4.4.3]
///
/// [RFC 7798 §4.4.3]: https://tools.ietf.org/html/rfc7798#section-4.4.3
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct H265FragmentationUnitPacket {
    /// payload_header is the header of the H265 packet.
    payload_header: H265NALUHeader,
    /// fu_header is the header of the fragmentation unit
    fu_header: H265FragmentationUnitHeader,
    /// donl is a 16-bit field, that may or may not be present.
    donl: Option<u16>,
    /// payload of the fragmentation unit.
    payload: Bytes,

    might_need_donl: bool,
}

impl H265FragmentationUnitPacket {
    /// with_donl can be called to specify whether or not DONL might be parsed.
    /// DONL may need to be parsed if `sprop-max-don-diff` is greater than 0 on the RTP stream.
    pub fn with_donl(&mut self, value: bool) {
        self.might_need_donl = value;
    }

    /// depacketize parses the passed byte slice and stores the result in the H265FragmentationUnitPacket this method is called upon.
    fn depacketize(&mut self, payload: &Bytes) -> Result<()> {
        const TOTAL_HEADER_SIZE: usize = H265NALU_HEADER_SIZE + H265FRAGMENTATION_UNIT_HEADER_SIZE;
        if payload.len() <= TOTAL_HEADER_SIZE {
            return Err(Error::ErrShortPacket);
        }

        let payload_header = H265NALUHeader::new(payload[0], payload[1]);
        if payload_header.f() {
            return Err(Error::ErrH265CorruptedPacket);
        }
        if !payload_header.is_fragmentation_unit() {
            return Err(Error::ErrInvalidH265PacketType);
        }

        let fu_header = H265FragmentationUnitHeader(payload[2]);
        let mut payload = payload.slice(3..);

        if fu_header.s() && self.might_need_donl {
            if payload.len() <= 2 {
                return Err(Error::ErrShortPacket);
            }

            let donl = ((payload[0] as u16) << 8) | (payload[1] as u16);
            self.donl = Some(donl);
            payload = payload.slice(2..);
        }

        self.payload_header = payload_header;
        self.fu_header = fu_header;
        self.payload = payload;

        Ok(())
    }

    /// payload_header returns the NALU header of the packet.
    pub fn payload_header(&self) -> H265NALUHeader {
        self.payload_header
    }

    /// fu_header returns the Fragmentation Unit Header of the packet.
    pub fn fu_header(&self) -> H265FragmentationUnitHeader {
        self.fu_header
    }

    /// donl returns the DONL of the packet.
    pub fn donl(&self) -> Option<u16> {
        self.donl
    }

    /// payload returns the Fragmentation Unit packet payload.
    pub fn payload(&self) -> Bytes {
        self.payload.clone()
    }
}

///
/// PACI implementation
///

/// H265PACIPacket represents a single H265 PACI packet.
///
///  0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |    PayloadHdr (Type=50)       |A|   cType   | phssize |F0..2|Y|
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |        payload Header Extension Structure (phes)              |
/// |=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=|
/// |                                                               |
/// |                  PACI payload: NAL unit                       |
/// |                   . . .                                       |
/// |                                                               |
/// |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                               :...OPTIONAL RTP padding        |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// ## Specifications
///
/// * [RFC 7798 §4.4.4]
///
/// [RFC 7798 §4.4.4]: https://tools.ietf.org/html/rfc7798#section-4.4.4
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct H265PACIPacket {
    /// payload_header is the header of the H265 packet.
    payload_header: H265NALUHeader,

    /// Field which holds value for `A`, `cType`, `phssize`, `F0`, `F1`, `F2` and `Y` fields.
    paci_header_fields: u16,

    /// phes is a header extension, of byte length `phssize`
    phes: Bytes,

    /// payload contains NAL units & optional padding
    payload: Bytes,
}

impl H265PACIPacket {
    /// payload_header returns the NAL Unit Header.
    pub fn payload_header(&self) -> H265NALUHeader {
        self.payload_header
    }

    /// a copies the F bit of the PACI payload NALU.
    pub fn a(&self) -> bool {
        const MASK: u16 = 0b10000000 << 8;
        (self.paci_header_fields & MASK) != 0
    }

    /// ctype copies the Type field of the PACI payload NALU.
    pub fn ctype(&self) -> u8 {
        const MASK: u16 = 0b01111110 << 8;
        ((self.paci_header_fields & MASK) >> (8 + 1)) as u8
    }

    /// phs_size indicates the size of the phes field.
    pub fn phs_size(&self) -> u8 {
        const MASK: u16 = (0b00000001 << 8) | 0b11110000;
        ((self.paci_header_fields & MASK) >> 4) as u8
    }

    /// f0 indicates the presence of a Temporal Scalability support extension in the phes.
    pub fn f0(&self) -> bool {
        const MASK: u16 = 0b00001000;
        (self.paci_header_fields & MASK) != 0
    }

    /// f1 must be zero, reserved for future extensions.
    pub fn f1(&self) -> bool {
        const MASK: u16 = 0b00000100;
        (self.paci_header_fields & MASK) != 0
    }

    /// f2 must be zero, reserved for future extensions.
    pub fn f2(&self) -> bool {
        const MASK: u16 = 0b00000010;
        (self.paci_header_fields & MASK) != 0
    }

    /// y must be zero, reserved for future extensions.
    pub fn y(&self) -> bool {
        const MASK: u16 = 0b00000001;
        (self.paci_header_fields & MASK) != 0
    }

    /// phes contains header extensions. Its size is indicated by phssize.
    pub fn phes(&self) -> Bytes {
        self.phes.clone()
    }

    /// payload is a single NALU or NALU-like struct, not including the first two octets (header).
    pub fn payload(&self) -> Bytes {
        self.payload.clone()
    }

    /// tsci returns the Temporal Scalability Control Information extension, if present.
    pub fn tsci(&self) -> Option<H265TSCI> {
        if !self.f0() || self.phs_size() < 3 {
            return None;
        }

        Some(H265TSCI(
            ((self.phes[0] as u32) << 16) | ((self.phes[1] as u32) << 8) | self.phes[0] as u32,
        ))
    }

    /// depacketize parses the passed byte slice and stores the result in the H265PACIPacket this method is called upon.
    fn depacketize(&mut self, payload: &Bytes) -> Result<()> {
        const TOTAL_HEADER_SIZE: usize = H265NALU_HEADER_SIZE + 2;
        if payload.len() <= TOTAL_HEADER_SIZE {
            return Err(Error::ErrShortPacket);
        }

        let payload_header = H265NALUHeader::new(payload[0], payload[1]);
        if payload_header.f() {
            return Err(Error::ErrH265CorruptedPacket);
        }
        if !payload_header.is_paci_packet() {
            return Err(Error::ErrInvalidH265PacketType);
        }

        let paci_header_fields = ((payload[2] as u16) << 8) | (payload[3] as u16);
        let mut payload = payload.slice(4..);

        self.paci_header_fields = paci_header_fields;
        let header_extension_size = self.phs_size();

        if payload.len() < header_extension_size as usize + 1 {
            self.paci_header_fields = 0;
            return Err(Error::ErrShortPacket);
        }

        self.payload_header = payload_header;

        if header_extension_size > 0 {
            self.phes = payload.slice(..header_extension_size as usize);
        }

        payload = payload.slice(header_extension_size as usize..);
        self.payload = payload;

        Ok(())
    }
}

///
/// Temporal Scalability Control Information
///

/// H265TSCI is a Temporal Scalability Control Information header extension.
///
/// ## Specifications
///
/// * [RFC 7798 §4.5]
///
/// [RFC 7798 §4.5]: https://tools.ietf.org/html/rfc7798#section-4.5
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct H265TSCI(pub u32);

impl H265TSCI {
    /// tl0picidx see RFC7798 for more details.
    pub fn tl0picidx(&self) -> u8 {
        const M1: u32 = 0xFFFF0000;
        const M2: u32 = 0xFF00;
        ((((self.0 & M1) >> 16) & M2) >> 8) as u8
    }

    /// irap_pic_id see RFC7798 for more details.
    pub fn irap_pic_id(&self) -> u8 {
        const M1: u32 = 0xFFFF0000;
        const M2: u32 = 0x00FF;
        (((self.0 & M1) >> 16) & M2) as u8
    }

    /// s see RFC7798 for more details.
    pub fn s(&self) -> bool {
        const M1: u32 = 0xFF00;
        const M2: u32 = 0b10000000;
        (((self.0 & M1) >> 8) & M2) != 0
    }

    /// e see RFC7798 for more details.
    pub fn e(&self) -> bool {
        const M1: u32 = 0xFF00;
        const M2: u32 = 0b01000000;
        (((self.0 & M1) >> 8) & M2) != 0
    }

    /// res see RFC7798 for more details.
    pub fn res(&self) -> u8 {
        const M1: u32 = 0xFF00;
        const M2: u32 = 0b00111111;
        (((self.0 & M1) >> 8) & M2) as u8
    }
}

///
/// H265 Payload Enum
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum H265Payload {
    H265SingleNALUnitPacket(H265SingleNALUnitPacket),
    H265FragmentationUnitPacket(H265FragmentationUnitPacket),
    H265AggregationPacket(H265AggregationPacket),
    H265PACIPacket(H265PACIPacket),
}

impl Default for H265Payload {
    fn default() -> Self {
        H265Payload::H265SingleNALUnitPacket(H265SingleNALUnitPacket::default())
    }
}

///
/// Packet implementation
///

/// H265Packet represents a H265 packet, stored in the payload of an RTP packet.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct H265Packet {
    payload: H265Payload,
    might_need_donl: bool,
}

impl H265Packet {
    /// with_donl can be called to specify whether or not DONL might be parsed.
    /// DONL may need to be parsed if `sprop-max-don-diff` is greater than 0 on the RTP stream.
    pub fn with_donl(&mut self, value: bool) {
        self.might_need_donl = value;
    }

    /// payload returns the populated payload.
    /// Must be casted to one of:
    /// - H265SingleNALUnitPacket
    /// - H265FragmentationUnitPacket
    /// - H265AggregationPacket
    /// - H265PACIPacket
    pub fn payload(&self) -> &H265Payload {
        &self.payload
    }
}

impl Depacketizer for H265Packet {
    /// depacketize parses the passed byte slice and stores the result in the H265Packet this method is called upon
    fn depacketize(&mut self, payload: &Bytes) -> Result<Bytes> {
        if payload.len() <= H265NALU_HEADER_SIZE {
            return Err(Error::ErrShortPacket);
        }

        let payload_header = H265NALUHeader::new(payload[0], payload[1]);
        if payload_header.f() {
            return Err(Error::ErrH265CorruptedPacket);
        }

        if payload_header.is_paci_packet() {
            let mut decoded = H265PACIPacket::default();
            decoded.depacketize(payload)?;

            self.payload = H265Payload::H265PACIPacket(decoded);
        } else if payload_header.is_fragmentation_unit() {
            let mut decoded = H265FragmentationUnitPacket::default();
            decoded.with_donl(self.might_need_donl);

            decoded.depacketize(payload)?;

            self.payload = H265Payload::H265FragmentationUnitPacket(decoded);
        } else if payload_header.is_aggregation_packet() {
            let mut decoded = H265AggregationPacket::default();
            decoded.with_donl(self.might_need_donl);

            decoded.depacketize(payload)?;

            self.payload = H265Payload::H265AggregationPacket(decoded);
        } else {
            let mut decoded = H265SingleNALUnitPacket::default();
            decoded.with_donl(self.might_need_donl);

            decoded.depacketize(payload)?;

            self.payload = H265Payload::H265SingleNALUnitPacket(decoded);
        }

        Ok(payload.clone())
    }

    /// is_partition_head checks if this is the head of a packetized nalu stream.
    fn is_partition_head(&self, _payload: &Bytes) -> bool {
        //TODO:
        true
    }

    fn is_partition_tail(&self, marker: bool, _payload: &Bytes) -> bool {
        marker
    }
}
