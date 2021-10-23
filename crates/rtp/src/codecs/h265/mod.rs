use bytes::Bytes;

#[cfg(test)]
mod h265_test;

///
/// Network Abstraction Unit Header implementation
///

const H265NALU_HEADER_SIZE: usize = 2;
// https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.2
const H265NALU_AGGREGATION_PACKET_TYPE: u8 = 48;
// https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.3
const H265NALU_FRAGMENTATION_UNIT_TYPE: u8 = 49;
// https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.4
const H265NALU_PACI_PACKET_TYPE: u8 = 50;

/// H265NALUHeader is a H265 NAL Unit Header
/// https://datatracker.ietf.org/doc/html/rfc7798#section-1.1.4
/// +---------------+---------------+
///  |0|1|2|3|4|5|6|7|0|1|2|3|4|5|6|7|
///  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///  |F|   Type    |  layer_id  | tid |
///  +-------------+-----------------+
#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct H265NALUHeader(pub u16);

impl H265NALUHeader {
    fn new(high_byte: u8, low_byte: u8) -> Self {
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
/// Reference: https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.1
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

    /*TODO:
    /// Unmarshal parses the passed byte slice and stores the result in the H265SingleNALUnitPacket this method is called upon.
    pub fn Unmarshal(payload []byte) ([]byte, error) {
        // sizeof(headers)
        const totalHeaderSize = H265NALU_HEADER_SIZE
        if payload == nil {
            return nil, errNilPacket
        } else if len(payload) <= totalHeaderSize {
            return nil, fmt.Errorf("%w: %d <= %v", errShortPacket, len(payload), totalHeaderSize)
        }

        payload_header := newH265NALUHeader(payload[0], payload[1])
        if payload_header.F() {
            return nil, errH265CorruptedPacket
        }
        if payload_header.is_fragmentation_unit() || payload_header.is_pacipacket() || payload_header.is_aggregation_packet() {
            return nil, errInvalidH265PacketType
        }

        payload = payload[2:]

        if p.might_need_donl {
            // sizeof(uint16)
            if len(payload) <= 2 {
                return nil, errShortPacket
            }

            donl := (uint16(payload[0]) << 8) | uint16(payload[1])
            p.donl = &donl
            payload = payload[2:]
        }

        p.payload_header = payload_header
        p.payload = payload

        return nil, nil
    }*/

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

    fn is_h265packet(&self) {}
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
/// Reference: https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.2
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
/*
// H265AggregationUnit represent the an Aggregation Unit in an AP, which is not the first one.
//
//    0                   1                   2                   3
//    0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//                   : DOND (cond)   |          NALU size            |
//   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//   |                                                               |
//   |                       NAL unit                                |
//   |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//   |                               :
//   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
// Reference: https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.2
type H265AggregationUnit struct {
    dond        *uint8
    nal_unit_size uint16
    nal_unit     []byte
}

// DOND field plus 1 specifies the difference between
// the decoding order number values of the current aggregated NAL unit
// and the preceding aggregated NAL unit in the same AP.
func (u H265AggregationUnit) DOND() *uint8 {
    return u.dond
}

// nalusize represents the size, in bytes, of the nal_unit.
func (u H265AggregationUnit) nalusize() uint16 {
    return u.nal_unit_size
}

// nal_unit payload.
func (u H265AggregationUnit) nal_unit() []byte {
    return u.nal_unit
}

// H265AggregationPacket represents an Aggregation packet.
//   0                   1                   2                   3
//    0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//   |    PayloadHdr (Type=48)       |                               |
//   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+                               |
//   |                                                               |
//   |             two or more aggregation units                     |
//   |                                                               |
//   |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//   |                               :...OPTIONAL RTP padding        |
//   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
// Reference: https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.2
type H265AggregationPacket struct {
    firstUnit  *H265AggregationUnitFirst
    otherUnits []H265AggregationUnit

    might_need_donl bool
}

// with_donl can be called to specify whether or not DONL might be parsed.
// DONL may need to be parsed if `sprop-max-don-diff` is greater than 0 on the RTP stream.
func (p *H265AggregationPacket) with_donl(value bool) {
    p.might_need_donl = value
}

// Unmarshal parses the passed byte slice and stores the result in the H265AggregationPacket this method is called upon.
func (p *H265AggregationPacket) Unmarshal(payload []byte) ([]byte, error) {
    // sizeof(headers)
    const totalHeaderSize = H265NALU_HEADER_SIZE
    if payload == nil {
        return nil, errNilPacket
    } else if len(payload) <= totalHeaderSize {
        return nil, fmt.Errorf("%w: %d <= %v", errShortPacket, len(payload), totalHeaderSize)
    }

    payload_header := newH265NALUHeader(payload[0], payload[1])
    if payload_header.F() {
        return nil, errH265CorruptedPacket
    }
    if !payload_header.is_aggregation_packet() {
        return nil, errInvalidH265PacketType
    }

    // First parse the first aggregation unit
    payload = payload[2:]
    firstUnit := &H265AggregationUnitFirst{}

    if p.might_need_donl {
        if len(payload) < 2 {
            return nil, errShortPacket
        }

        donl := (uint16(payload[0]) << 8) | uint16(payload[1])
        firstUnit.donl = &donl

        payload = payload[2:]
    }
    if len(payload) < 2 {
        return nil, errShortPacket
    }
    firstUnit.nal_unit_size = (uint16(payload[0]) << 8) | uint16(payload[1])
    payload = payload[2:]

    if len(payload) < int(firstUnit.nal_unit_size) {
        return nil, errShortPacket
    }

    firstUnit.nal_unit = payload[:firstUnit.nal_unit_size]
    payload = payload[firstUnit.nal_unit_size:]

    // Parse remaining Aggregation Units
    var units []H265AggregationUnit
    for {
        unit := H265AggregationUnit{}

        if p.might_need_donl {
            if len(payload) < 1 {
                break
            }

            dond := payload[0]
            unit.dond = &dond

            payload = payload[1:]
        }

        if len(payload) < 2 {
            break
        }
        unit.nal_unit_size = (uint16(payload[0]) << 8) | uint16(payload[1])
        payload = payload[2:]

        if len(payload) < int(unit.nal_unit_size) {
            break
        }

        unit.nal_unit = payload[:unit.nal_unit_size]
        payload = payload[unit.nal_unit_size:]

        units = append(units, unit)
    }

    // There need to be **at least** two Aggregation Units (first + another one)
    if len(units) == 0 {
        return nil, errShortPacket
    }

    p.firstUnit = firstUnit
    p.otherUnits = units

    return nil, nil
}

// FirstUnit returns the first Aggregated Unit of the packet.
func (p *H265AggregationPacket) FirstUnit() *H265AggregationUnitFirst {
    return p.firstUnit
}

// OtherUnits returns the all the other Aggregated Unit of the packet (excluding the first one).
func (p *H265AggregationPacket) OtherUnits() []H265AggregationUnit {
    return p.otherUnits
}

func (p *H265AggregationPacket) is_h265packet() {}

//
// Fragmentation Unit implementation
//

const (
    // sizeof(uint8)
    h265FragmentationUnitHeaderSize = 1
)

// H265FragmentationUnitHeader is a H265 FU Header
// +---------------+
// |0|1|2|3|4|5|6|7|
// +-+-+-+-+-+-+-+-+
// |S|E|  FuType   |
// +---------------+
type H265FragmentationUnitHeader uint8

// S represents the start of a fragmented NAL unit.
func (h H265FragmentationUnitHeader) S() bool {
    const mask = 0b10000000
    return ((h & mask) >> 7) != 0
}

// E represents the end of a fragmented NAL unit.
func (h H265FragmentationUnitHeader) E() bool {
    const mask = 0b01000000
    return ((h & mask) >> 6) != 0
}

// FuType MUST be equal to the field Type of the fragmented NAL unit.
func (h H265FragmentationUnitHeader) FuType() uint8 {
    const mask = 0b00111111
    return uint8(h) & mask
}

// H265FragmentationUnitPacket represents a single Fragmentation Unit packet.
//
//  0                   1                   2                   3
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |    PayloadHdr (Type=49)       |   FU header   | DONL (cond)   |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-|
// | DONL (cond)   |                                               |
// |-+-+-+-+-+-+-+-+                                               |
// |                         FU payload                            |
// |                                                               |
// |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                               :...OPTIONAL RTP padding        |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
// Reference: https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.3
type H265FragmentationUnitPacket struct {
    // payload_header is the header of the H265 packet.
    payload_header H265NALUHeader
    // fuHeader is the header of the fragmentation unit
    fuHeader H265FragmentationUnitHeader
    // donl is a 16-bit field, that may or may not be present.
    donl *uint16
    // payload of the fragmentation unit.
    payload []byte

    might_need_donl bool
}

// with_donl can be called to specify whether or not DONL might be parsed.
// DONL may need to be parsed if `sprop-max-don-diff` is greater than 0 on the RTP stream.
func (p *H265FragmentationUnitPacket) with_donl(value bool) {
    p.might_need_donl = value
}

// Unmarshal parses the passed byte slice and stores the result in the H265FragmentationUnitPacket this method is called upon.
func (p *H265FragmentationUnitPacket) Unmarshal(payload []byte) ([]byte, error) {
    // sizeof(headers)
    const totalHeaderSize = H265NALU_HEADER_SIZE + h265FragmentationUnitHeaderSize
    if payload == nil {
        return nil, errNilPacket
    } else if len(payload) <= totalHeaderSize {
        return nil, fmt.Errorf("%w: %d <= %v", errShortPacket, len(payload), totalHeaderSize)
    }

    payload_header := newH265NALUHeader(payload[0], payload[1])
    if payload_header.F() {
        return nil, errH265CorruptedPacket
    }
    if !payload_header.is_fragmentation_unit() {
        return nil, errInvalidH265PacketType
    }

    fuHeader := H265FragmentationUnitHeader(payload[2])
    payload = payload[3:]

    if fuHeader.S() && p.might_need_donl {
        // sizeof(uint16)
        if len(payload) <= 2 {
            return nil, errShortPacket
        }

        donl := (uint16(payload[0]) << 8) | uint16(payload[1])
        p.donl = &donl
        payload = payload[2:]
    }

    p.payload_header = payload_header
    p.fuHeader = fuHeader
    p.payload = payload

    return nil, nil
}

// payload_header returns the NALU header of the packet.
func (p *H265FragmentationUnitPacket) payload_header() H265NALUHeader {
    return p.payload_header
}

// FuHeader returns the Fragmentation Unit Header of the packet.
func (p *H265FragmentationUnitPacket) FuHeader() H265FragmentationUnitHeader {
    return p.fuHeader
}

// DONL returns the DONL of the packet.
func (p *H265FragmentationUnitPacket) DONL() *uint16 {
    return p.donl
}

// payload returns the Fragmentation Unit packet payload.
func (p *H265FragmentationUnitPacket) payload() []byte {
    return p.payload
}

func (p *H265FragmentationUnitPacket) is_h265packet() {}

//
// PACI implementation
//

// H265PACIPacket represents a single H265 PACI packet.
//
//  0                   1                   2                   3
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |    PayloadHdr (Type=50)       |A|   cType   | PHSsize |F0..2|Y|
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |        payload Header Extension Structure (PHES)              |
// |=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=|
// |                                                               |
// |                  PACI payload: NAL unit                       |
// |                   . . .                                       |
// |                                                               |
// |                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                               :...OPTIONAL RTP padding        |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
// Reference: https://datatracker.ietf.org/doc/html/rfc7798#section-4.4.4
type H265PACIPacket struct {
    // payload_header is the header of the H265 packet.
    payload_header H265NALUHeader

    // Field which holds value for `A`, `cType`, `PHSsize`, `F0`, `F1`, `F2` and `Y` fields.
    paciHeaderFields uint16

    // phes is a header extension, of byte length `PHSsize`
    phes []byte

    // payload contains NAL units & optional padding
    payload []byte
}

// payload_header returns the NAL Unit Header.
func (p *H265PACIPacket) payload_header() H265NALUHeader {
    return p.payload_header
}

// A copies the F bit of the PACI payload NALU.
func (p *H265PACIPacket) A() bool {
    const mask = 0b10000000 << 8
    return (p.paciHeaderFields & mask) != 0
}

// CType copies the Type field of the PACI payload NALU.
func (p *H265PACIPacket) CType() uint8 {
    const mask = 0b01111110 << 8
    return uint8((p.paciHeaderFields & mask) >> (8 + 1))
}

// PHSsize indicates the size of the PHES field.
func (p *H265PACIPacket) PHSsize() uint8 {
    const mask = (0b00000001 << 8) | 0b11110000
    return uint8((p.paciHeaderFields & mask) >> 4)
}

// F0 indicates the presence of a Temporal Scalability support extension in the PHES.
func (p *H265PACIPacket) F0() bool {
    const mask = 0b00001000
    return (p.paciHeaderFields & mask) != 0
}

// F1 must be zero, reserved for future extensions.
func (p *H265PACIPacket) F1() bool {
    const mask = 0b00000100
    return (p.paciHeaderFields & mask) != 0
}

// F2 must be zero, reserved for future extensions.
func (p *H265PACIPacket) F2() bool {
    const mask = 0b00000010
    return (p.paciHeaderFields & mask) != 0
}

// Y must be zero, reserved for future extensions.
func (p *H265PACIPacket) Y() bool {
    const mask = 0b00000001
    return (p.paciHeaderFields & mask) != 0
}

// PHES contains header extensions. Its size is indicated by PHSsize.
func (p *H265PACIPacket) PHES() []byte {
    return p.phes
}

// payload is a single NALU or NALU-like struct, not including the first two octets (header).
func (p *H265PACIPacket) payload() []byte {
    return p.payload
}

// TSCI returns the Temporal Scalability Control Information extension, if present.
func (p *H265PACIPacket) TSCI() *H265TSCI {
    if !p.F0() || p.PHSsize() < 3 {
        return nil
    }

    tsci := H265TSCI((uint32(p.phes[0]) << 16) | (uint32(p.phes[1]) << 8) | uint32(p.phes[0]))
    return &tsci
}

// Unmarshal parses the passed byte slice and stores the result in the H265PACIPacket this method is called upon.
func (p *H265PACIPacket) Unmarshal(payload []byte) ([]byte, error) {
    // sizeof(headers)
    const totalHeaderSize = H265NALU_HEADER_SIZE + 2
    if payload == nil {
        return nil, errNilPacket
    } else if len(payload) <= totalHeaderSize {
        return nil, fmt.Errorf("%w: %d <= %v", errShortPacket, len(payload), totalHeaderSize)
    }

    payload_header := newH265NALUHeader(payload[0], payload[1])
    if payload_header.F() {
        return nil, errH265CorruptedPacket
    }
    if !payload_header.is_pacipacket() {
        return nil, errInvalidH265PacketType
    }

    paciHeaderFields := (uint16(payload[2]) << 8) | uint16(payload[3])
    payload = payload[4:]

    p.paciHeaderFields = paciHeaderFields
    headerExtensionSize := p.PHSsize()

    if len(payload) < int(headerExtensionSize)+1 {
        p.paciHeaderFields = 0
        return nil, errShortPacket
    }

    p.payload_header = payload_header

    if headerExtensionSize > 0 {
        p.phes = payload[:headerExtensionSize]
    }

    payload = payload[headerExtensionSize:]
    p.payload = payload

    return nil, nil
}

func (p *H265PACIPacket) is_h265packet() {}

//
// Temporal Scalability Control Information
//

// H265TSCI is a Temporal Scalability Control Information header extension.
// Reference: https://datatracker.ietf.org/doc/html/rfc7798#section-4.5
type H265TSCI uint32

// TL0PICIDX see RFC7798 for more details.
func (h H265TSCI) TL0PICIDX() uint8 {
    const m1 = 0xFFFF0000
    const m2 = 0xFF00
    return uint8((((h & m1) >> 16) & m2) >> 8)
}

// IrapPicID see RFC7798 for more details.
func (h H265TSCI) IrapPicID() uint8 {
    const m1 = 0xFFFF0000
    const m2 = 0x00FF
    return uint8(((h & m1) >> 16) & m2)
}

// S see RFC7798 for more details.
func (h H265TSCI) S() bool {
    const m1 = 0xFF00
    const m2 = 0b10000000
    return (uint8((h&m1)>>8) & m2) != 0
}

// E see RFC7798 for more details.
func (h H265TSCI) E() bool {
    const m1 = 0xFF00
    const m2 = 0b01000000
    return (uint8((h&m1)>>8) & m2) != 0
}

// RES see RFC7798 for more details.
func (h H265TSCI) RES() uint8 {
    const m1 = 0xFF00
    const m2 = 0b00111111
    return uint8((h&m1)>>8) & m2
}

//
// H265 Packet interface
//

type is_h265packet interface {
    is_h265packet()
}

var (
    _ is_h265packet = (*H265FragmentationUnitPacket)(nil)
    _ is_h265packet = (*H265PACIPacket)(nil)
    _ is_h265packet = (*H265SingleNALUnitPacket)(nil)
    _ is_h265packet = (*H265AggregationPacket)(nil)
)

//
// Packet implementation
//

// H265Packet represents a H265 packet, stored in the payload of an RTP packet.
type H265Packet struct {
    packet        is_h265packet
    might_need_donl bool
}

// with_donl can be called to specify whether or not DONL might be parsed.
// DONL may need to be parsed if `sprop-max-don-diff` is greater than 0 on the RTP stream.
func (p *H265Packet) with_donl(value bool) {
    p.might_need_donl = value
}

// Unmarshal parses the passed byte slice and stores the result in the H265Packet this method is called upon
func (p *H265Packet) Unmarshal(payload []byte) ([]byte, error) {
    if payload == nil {
        return nil, errNilPacket
    } else if len(payload) <= H265NALU_HEADER_SIZE {
        return nil, fmt.Errorf("%w: %d <= %v", errShortPacket, len(payload), H265NALU_HEADER_SIZE)
    }

    payload_header := newH265NALUHeader(payload[0], payload[1])
    if payload_header.F() {
        return nil, errH265CorruptedPacket
    }

    switch {
    case payload_header.is_pacipacket():
        decoded := &H265PACIPacket{}
        if _, err := decoded.Unmarshal(payload); err != nil {
            return nil, err
        }

        p.packet = decoded

    case payload_header.is_fragmentation_unit():
        decoded := &H265FragmentationUnitPacket{}
        decoded.with_donl(p.might_need_donl)

        if _, err := decoded.Unmarshal(payload); err != nil {
            return nil, err
        }

        p.packet = decoded

    case payload_header.is_aggregation_packet():
        decoded := &H265AggregationPacket{}
        decoded.with_donl(p.might_need_donl)

        if _, err := decoded.Unmarshal(payload); err != nil {
            return nil, err
        }

        p.packet = decoded

    default:
        decoded := &H265SingleNALUnitPacket{}
        decoded.with_donl(p.might_need_donl)

        if _, err := decoded.Unmarshal(payload); err != nil {
            return nil, err
        }

        p.packet = decoded
    }

    return nil, nil
}

// Packet returns the populated packet.
// Must be casted to one of:
// - *H265SingleNALUnitPacket
// - *H265FragmentationUnitPacket
// - *H265AggregationPacket
// - *H265PACIPacket
// nolint:golint
func (p *H265Packet) Packet() is_h265packet {
    return p.packet
}
*/
