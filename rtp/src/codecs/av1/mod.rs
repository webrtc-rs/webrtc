use std::cmp::min;
use std::fmt::format;

use bytes::{BufMut, Bytes, BytesMut};

use crate::packetizer::Payloader;

#[derive(Default, Clone, Debug)]
pub struct Av1Payloader {}

const AGGREGATION_HEADER_SIZE: usize = 1;

// when there are 3 or less OBU (fragments) in a packet, size of the last one
// can be omited.
const MAX_NUM_OBUS_TO_OMIT_SIZE: usize = 3;

const OBU_SIZE_PRESENT_BIT: u8 = 0b0_0000_010;

const OBU_TYPE_SEQUENCE_HEADER: u8 = 1;
const OBU_TYPE_TEMPORAL_DELIMITER: u8 = 2;
const OBU_TYPE_TILE_LIST: u8 = 3;
const OBU_TYPE_PADDING: u8 = 15;

fn encode_leb128(mut val: u32) -> u32 {
    let mut b = 0;
    loop {
        b |= val & 0b01111111;
        val >>= 7;
        if val != 0 {
            b |= 0x80;
            b <<= 8;
        } else {
            return b;
        }
    }
}

fn decode_leb128(mut val: u32) -> u32 {
    let mut b = 0;
    loop {
        b |= val & 0b01111111;
        val >>= 8;
        if val == 0 {
            return b;
        }
        b <<= 7;
    }
}

fn read_leb128(bytes: &Bytes) -> (u32, usize) {
    let mut encoded = 0;
    for i in 0..bytes.len() {
        encoded |= bytes[i] as u32;
        if bytes[i] & 0b1000_0000 == 0 {
            return (decode_leb128(encoded), i + 1);
        }
        encoded <<= 8;
    }
    (0, 0)
}

fn leb128_size(value: u32) -> usize {
    let mut size = 0;
    let mut value = value;
    while value >= 0x80 {
        size += 1;
        value >>= 7;
    }
    size + 1
}

fn max_fragment_size(remaining_bytes: usize) -> usize {
    if remaining_bytes <= 1 {
        return 0;
    }
    let mut i = 1;
    loop {
        if remaining_bytes < (1 << 7 * i) + i {
            return remaining_bytes - i;
        }
        i += 1;
    }
}

struct Obu {
    header: u8,
    extension_header: u8,
    payload: Bytes,
    size: usize,
}

fn obu_has_extension(header: u8) -> bool {
    header & 0b0_0000_100 != 0
}

fn obu_has_size(header: u8) -> bool {
    header & 0b0_0000_010 != 0
}

fn obu_type(header: u8) -> u8 {
    (header & 0b0_1111_000) >> 3
}

struct Packet {
    first_obu_index: usize,
    num_obu_elements: usize,
    first_obu_offset: usize,
    last_obu_size: usize,
    packet_size: usize,
}

impl Packet {
    fn new(first_obu_index: usize) -> Self {
        Self {
            first_obu_index,
            num_obu_elements: 0,
            first_obu_offset: 0,
            last_obu_size: 0,
            packet_size: 0,
        }
    }
}

fn parse_obus(payload: &Bytes) -> Vec<Obu> {
    let mut obus = vec![];
    let mut consumed = 0;

    while consumed < payload.len() {
        let payload_remaining = payload.slice(consumed..);
        let header = payload_remaining[0];
        consumed += 1;
        let mut obu_size = 1;
        let extension_header = if obu_has_extension(header) {
            if payload_remaining.len() < 2 {
                // TODO Err "Payload too small for OBU extension header";
                return vec![];
            }
            obu_size += 1;
            consumed += 1;
            payload_remaining[1]
        } else {
            0
        };

        let payload_without_header = payload_remaining.slice(obu_size..);
        let obu_payload = if !obu_has_size(header) {
            payload_without_header
        } else {
            if payload_without_header.len() < 1 {
                // TODO Err "Payload too small for OBU size";
                return vec![];
            }
            let (size, size_encoding_size) = read_leb128(&payload_without_header);
            consumed += size_encoding_size + size as usize;
            payload_without_header.slice(size_encoding_size..size_encoding_size + size as usize)
            // payload_remaining.slice(obu_size..obu_size + size as usize)
        };
        obu_size += obu_payload.len();

        let obu_type = obu_type(header);
        if obu_type != OBU_TYPE_TEMPORAL_DELIMITER
            && obu_type != OBU_TYPE_TILE_LIST
            && obu_type != OBU_TYPE_PADDING
        {
            obus.push(Obu {
                header,
                extension_header,
                payload: obu_payload,
                size: obu_size,
            });
        }
    }

    obus
}

fn additional_bytes_for_previous_obu_element(packet: &Packet) -> usize {
    if packet.packet_size == 0 {
        // Packet is still empty => no last OBU element, no need to reserve space
        // for it.
        0
    } else if packet.num_obu_elements > MAX_NUM_OBUS_TO_OMIT_SIZE {
        // There are so many obu elements in the packet, all of them must be
        // prepended with the length field. That imply space for the length of the
        // last obu element is already reserved.
        0
    } else {
        leb128_size(packet.last_obu_size as u32)
    }
}

fn packetize(obus: &Vec<Obu>, max_payload_size: usize) -> Vec<Packet> {
    let mut packets = vec![];
    if obus.is_empty() {
        return packets;
    }
    if max_payload_size < 3 {
        // TODO Err "Payload size too small";
        return packets;
    }
    // Aggregation header will be present in all packets.
    let max_payload_size = max_payload_size - AGGREGATION_HEADER_SIZE;

    // Assemble packets. Push to current packet as much as it can hold before
    // considering next one. That would normally cause uneven distribution across
    // packets, specifically last one would be generally smaller.
    packets.push(Packet::new(0));
    let mut packet_remaining_bytes = max_payload_size;

    for obu_index in 0..obus.len() {
        let is_last_obu = obu_index == obus.len() - 1;
        let obu = &obus[obu_index];

        // Putting |obu| into the last packet would make last obu element stored in
        // that packet not last. All not last OBU elements must be prepend with the
        // element length. AdditionalBytesForPreviousObuElement calculates how many
        // bytes are needed to store that length.
        let mut previous_obu_extra_size =
            additional_bytes_for_previous_obu_element(packets.last().unwrap());
        let min_required_size =
            if packets.last().unwrap().num_obu_elements >= MAX_NUM_OBUS_TO_OMIT_SIZE {
                2
            } else {
                1
            };
        if packet_remaining_bytes < previous_obu_extra_size + min_required_size {
            // Start a new packet.
            packets.push(Packet::new(obu_index));
            packet_remaining_bytes = max_payload_size;
            previous_obu_extra_size = 0;
        }
        let mut packet = packets.pop().unwrap();
        packet.packet_size += previous_obu_extra_size;
        packet_remaining_bytes -= previous_obu_extra_size;
        packet.num_obu_elements += 1;
        let must_write_obu_element_size = packet.num_obu_elements > MAX_NUM_OBUS_TO_OMIT_SIZE;

        // Can fit all of the obu into the packet?
        let mut required_bytes = obu.size;
        if must_write_obu_element_size {
            required_bytes += leb128_size(obu.size as u32);
        }
        if required_bytes < packet_remaining_bytes {
            // Insert the obu into the packet unfragmented.
            packet.last_obu_size = obu.size;
            packet.packet_size += required_bytes;
            packet_remaining_bytes -= required_bytes;
            packets.push(packet);
            continue;
        }
        // Fragment the obu.
        let max_first_fragment_size = if must_write_obu_element_size {
            max_fragment_size(packet_remaining_bytes)
        } else {
            packet_remaining_bytes
        };
        // Because available_bytes might be different than
        // packet_remaining_bytes it might happen that max_first_fragment_size >=
        // obu.size. Also, since checks above verified |obu| should not be put
        // completely into the |packet|, leave at least 1 byte for later packet.
        let first_fragment_size = min(obu.size - 1, max_first_fragment_size);
        if first_fragment_size == 0 {
            // Rather than writing 0-size element at the tail of the packet,
            // 'uninsert' the |obu| from the |packet|.
            packet.num_obu_elements -= 1;
            packet.packet_size -= previous_obu_extra_size;
        } else {
            packet.packet_size += first_fragment_size;
            if must_write_obu_element_size {
                packet.packet_size += leb128_size(first_fragment_size as u32);
            }
            packet.last_obu_size = first_fragment_size;
        }
        packets.push(packet);
        // Add middle fragments that occupy all of the packet.
        // These are easy because
        // - one obu per packet imply no need to store the size of the obu.
        // - this packets are nor the first nor the last packets of the frame, so
        // packet capacity is always limits.max_payload_len.
        let mut obu_offset = first_fragment_size;
        while obu_offset + max_payload_size < obu.size {
            let mut packet = Packet::new(obu_index);
            packet.num_obu_elements = 1;
            packet.first_obu_offset = obu_offset;
            let middle_fragment_size = max_payload_size;
            packet.last_obu_size = middle_fragment_size;
            packet.packet_size = middle_fragment_size;
            packets.push(packet);
            obu_offset += max_payload_size;
        }
        // Add the last fragment of the obu.
        let mut last_fragment_size = obu.size - obu_offset;
        // Check for corner case where last fragment of the last obu is too large
        // to fit into last packet, but may fully fit into semi-last packet.
        if is_last_obu && last_fragment_size > max_payload_size {
            // Split last fragments into two.
            // Try to even packet sizes rather than payload sizes across the last
            // two packets.
            let mut semi_last_fragment_size = last_fragment_size / 2;
            // But leave at least one payload byte for the last packet to avoid
            // weird scenarios where size of the fragment is zero and rtp payload has
            // nothing except for an aggregation header.
            if semi_last_fragment_size >= last_fragment_size {
                semi_last_fragment_size = last_fragment_size - 1;
            }
            last_fragment_size -= semi_last_fragment_size;
            let mut packet = Packet::new(obu_index);
            packet.first_obu_offset = obu_offset;
            packet.last_obu_size = semi_last_fragment_size;
            packet.packet_size = semi_last_fragment_size;
            packets.push(packet);
            obu_offset += semi_last_fragment_size
        }
        let mut last_packet = Packet::new(obu_index);
        last_packet.num_obu_elements = 1;
        last_packet.first_obu_offset = obu_offset;
        last_packet.last_obu_size = last_fragment_size;
        last_packet.packet_size = last_fragment_size;
        packets.push(last_packet);
        packet_remaining_bytes = max_payload_size - last_fragment_size;
    }

    packets
}

fn get_aggregation_header(obus: &Vec<Obu>, packets: &Vec<Packet>, packet_index: usize) -> u8 {
    let packet = &packets[packet_index];
    let mut header: u8 = 0;

    // Set Z flag: first obu element is continuation of the previous OBU.
    let first_obu_element_is_fragment = packet.first_obu_offset > 0;
    if first_obu_element_is_fragment {
        header |= 1 << 7;
    }

    // Set Y flag: last obu element will be continuated in the next packet.
    let last_obu_offset = if packet.num_obu_elements == 1 {
        packet.first_obu_offset
    } else {
        0
    };
    let last_obu_is_fragment = last_obu_offset + packet.last_obu_size
        < obus[packet.first_obu_index + packet.num_obu_elements - 1].size;
    if last_obu_is_fragment {
        header |= 1 << 6;
    }

    // Set W field: number of obu elements in the packet (when not too large).
    if packet.num_obu_elements <= MAX_NUM_OBUS_TO_OMIT_SIZE {
        header |= (packet.num_obu_elements as u8) << 4;
    }

    // Set N flag: beginning of a new coded video sequence.
    // Encoder may produce key frame without a sequence header, thus double check
    // incoming frame includes the sequence header. Since Temporal delimiter is
    // already filtered out, sequence header should be the first obu when present.
    if packet_index == 0 && obu_type(obus.first().unwrap().header) == OBU_TYPE_SEQUENCE_HEADER {
        header |= 1 << 3;
    }
    header
}

impl Payloader for Av1Payloader {
    fn payload(&mut self, mtu: usize, payload: &Bytes) -> crate::error::Result<Vec<Bytes>> {
        let obus = parse_obus(payload);
        let packets = packetize(&obus, mtu);
        let mut payloads = vec![];
        for packet_index in 0..packets.len() {
            let packet = &packets[packet_index];
            let mut out = BytesMut::with_capacity(AGGREGATION_HEADER_SIZE + packet.packet_size);
            let aggregation_header = get_aggregation_header(&obus, &packets, packet_index);
            out.put_u8(aggregation_header);
            let mut obu_offset = packet.first_obu_offset;
            // Store all OBU elements except the last one.
            for i in 0..packet.num_obu_elements - 1 {
                let obu = &obus[packet.first_obu_index + i];
                let fragment_size = obu.size - obu_offset;
                let leb128_fragment_size = encode_leb128(fragment_size as u32);
                let leb128_fragment_size_size = leb128_size(fragment_size as u32);
                for i in 0..leb128_fragment_size_size {
                    out.put_u8(
                        (leb128_fragment_size >> ((leb128_fragment_size_size - 1 - i) * 8)) as u8,
                    )
                }
                if obu_offset == 0 {
                    out.put_u8(obu.header & !OBU_SIZE_PRESENT_BIT);
                }
                if obu_offset <= 1 && obu_has_extension(obu.header) {
                    out.put_u8(obu.extension_header);
                }
                let payload_offset = {
                    let headers_size = if obu_has_extension(obu.header) { 2 } else { 1 };
                    if obu_offset <= headers_size {
                        0
                    } else {
                        obu_offset - headers_size
                    }
                };
                let payload_size = obu.payload.len() - payload_offset;
                out.put_slice(
                    obu.payload
                        .slice(payload_offset..payload_offset + payload_size)
                        .as_ref(),
                );
                // All obus are stored from the beginning, except, may be, the first one.
                obu_offset = 0;
            }

            // Store the last OBU element.
            let last_obu = &obus[packet.first_obu_index + packet.num_obu_elements - 1];
            let mut fragment_size = packet.last_obu_size;
            if packet.num_obu_elements > MAX_NUM_OBUS_TO_OMIT_SIZE {
                let leb128_fragment_size = encode_leb128(fragment_size as u32);
                let leb128_fragment_size_size = leb128_size(fragment_size as u32);
                for i in 0..leb128_fragment_size_size {
                    out.put_u8(
                        (leb128_fragment_size >> ((leb128_fragment_size_size - 1 - i) * 8)) as u8,
                    )
                }
            }
            if obu_offset == 0 && fragment_size > 0 {
                out.put_u8(last_obu.header & !OBU_SIZE_PRESENT_BIT);
                fragment_size -= 1;
            }
            if obu_offset <= 1 && obu_has_extension(last_obu.header) && fragment_size > 0 {
                out.put_u8(last_obu.extension_header);
                fragment_size -= 1;
            }
            let payload_offset = {
                let headers_size = if obu_has_extension(last_obu.header) {
                    2
                } else {
                    1
                };
                if obu_offset <= headers_size {
                    0
                } else {
                    obu_offset - headers_size
                }
            };
            out.put_slice(
                last_obu
                    .payload
                    .slice(payload_offset..payload_offset + fragment_size)
                    .as_ref(),
            );
            payloads.push(out.freeze());
        }
        Ok(payloads)
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(self.clone())
    }
}
