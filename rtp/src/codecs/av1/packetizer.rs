//! Based on https://chromium.googlesource.com/external/webrtc/+/4e513346ec56c829b3a6010664998469fc237b35/modules/rtp_rtcp/source/rtp_packetizer_av1.cc
//! Reference: https://aomediacodec.github.io/av1-rtp-spec

use std::cmp::min;

use crate::codecs::av1::leb128::leb128_size;
use crate::codecs::av1::obu::{obu_type, Obu, OBU_TYPE_SEQUENCE_HEADER};

/// When there are 3 or less OBU (fragments) in a packet, size of the last one
/// can be omitted.
pub const MAX_NUM_OBUS_TO_OMIT_SIZE: usize = 3;
pub const AGGREGATION_HEADER_SIZE: usize = 1;

pub struct PacketMetadata {
    pub first_obu_index: usize,
    pub num_obu_elements: usize,
    pub first_obu_offset: usize,
    pub last_obu_size: usize,
    /// Total size consumed by the packet.
    pub packet_size: usize,
}

impl PacketMetadata {
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

/// Returns the scheme for how to aggregate or split the OBUs across RTP packets.
/// Reference: https://aomediacodec.github.io/av1-rtp-spec/#45-payload-structure
///            https://aomediacodec.github.io/av1-rtp-spec/#5-packetization-rules
pub fn packetize(obus: &[Obu], mtu: usize) -> Vec<PacketMetadata> {
    if obus.is_empty() {
        return vec![];
    }
    // Ignore certain edge cases where packets should be very small. They are
    // impractical but adds complexity to handle.
    if mtu < 3 {
        return vec![];
    }

    let mut packets = vec![];

    // Aggregation header will be present in all packets.
    let max_payload_size = mtu - AGGREGATION_HEADER_SIZE;

    // Assemble packets. Push to current packet as much as it can hold before
    // considering next one. That would normally cause uneven distribution across
    // packets, specifically last one would be generally smaller.
    packets.push(PacketMetadata::new(0));
    let mut packet_remaining_bytes = max_payload_size;

    for obu_index in 0..obus.len() {
        let is_last_obu = obu_index == obus.len() - 1;
        let obu = &obus[obu_index];

        // Putting |obu| into the last packet would make last obu element stored in
        // that packet not last. All not last OBU elements must be prepend with the
        // element length. AdditionalBytesForPreviousObuElement calculates how many
        // bytes are needed to store that length.
        let mut packet = packets.pop().unwrap();
        let mut previous_obu_extra_size = additional_bytes_for_previous_obu_element(&packet);
        let min_required_size = if packet.num_obu_elements >= MAX_NUM_OBUS_TO_OMIT_SIZE {
            2
        } else {
            1
        };
        if packet_remaining_bytes < previous_obu_extra_size + min_required_size {
            // Start a new packet.
            packets.push(packet);
            packet = PacketMetadata::new(obu_index);
            packet_remaining_bytes = max_payload_size;
            previous_obu_extra_size = 0;
        }
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
            let mut packet = PacketMetadata::new(obu_index);
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
            let mut packet = PacketMetadata::new(obu_index);
            packet.first_obu_offset = obu_offset;
            packet.last_obu_size = semi_last_fragment_size;
            packet.packet_size = semi_last_fragment_size;
            packets.push(packet);
            obu_offset += semi_last_fragment_size
        }
        let mut last_packet = PacketMetadata::new(obu_index);
        last_packet.num_obu_elements = 1;
        last_packet.first_obu_offset = obu_offset;
        last_packet.last_obu_size = last_fragment_size;
        last_packet.packet_size = last_fragment_size;
        packets.push(last_packet);
        packet_remaining_bytes = max_payload_size - last_fragment_size;
    }

    packets
}

/// Returns the aggregation header for the packet.
/// Reference: https://aomediacodec.github.io/av1-rtp-spec/#44-av1-aggregation-header
pub fn get_aggregation_header(obus: &[Obu], packets: &[PacketMetadata], packet_index: usize) -> u8 {
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
    //
    // TODO: This is technically incorrect, since sequence headers may be present in delta frames.
    //      However, unlike the Chromium implementation: https://chromium.googlesource.com/external/webrtc/+/4e513346ec56c829b3a6010664998469fc237b35/modules/rtp_rtcp/source/rtp_packetizer_av1.cc#345,
    //      we do not have direct access to the whether this is a keyframe or a delta frame.
    //      Thus for now we assume that every frame that starts with a sequence header is a keyframe,
    //      which is not always true. This is the best we can do for now until implementing
    //      a proper frame type detection, perhaps by parsing the FRAME_HEADER OBUs according to
    //      https://aomediacodec.github.io/av1-spec/#ordering-of-obus:
    //              A new coded video sequence is defined to start at each temporal unit which
    //              satisfies both of the following conditions:
    //                  - A sequence header OBU appears before the first frame header.
    //                  - The first frame header has frame_type equal to KEY_FRAME, show_frame equal
    //                    to 1, show_existing_frame equal to 0, and temporal_id equal to 0.
    if packet_index == 0 && obu_type(obus.first().unwrap().header) == OBU_TYPE_SEQUENCE_HEADER {
        header |= 1 << 3;
    }
    header
}

/// Returns the number of additional bytes needed to store the previous OBU
/// element if an additional OBU element is added to the packet.
fn additional_bytes_for_previous_obu_element(packet: &PacketMetadata) -> usize {
    if packet.packet_size == 0 || packet.num_obu_elements > MAX_NUM_OBUS_TO_OMIT_SIZE {
        // Packet is still empty => no last OBU element, no need to reserve space for it.
        //  OR
        // There are so many obu elements in the packet, all of them must be
        // prepended with the length field. That imply space for the length of the
        // last obu element is already reserved.
        0
    } else {
        leb128_size(packet.last_obu_size as u32)
    }
}

/// Given |remaining_bytes| free bytes left in a packet, returns max size of an
/// OBU fragment that can fit into the packet.
/// i.e. MaxFragmentSize + Leb128Size(MaxFragmentSize) <= remaining_bytes.
fn max_fragment_size(remaining_bytes: usize) -> usize {
    if remaining_bytes <= 1 {
        return 0;
    }
    let mut i = 1;
    loop {
        if remaining_bytes < (1 << (7 * i)) + i {
            return remaining_bytes - i;
        }
        i += 1;
    }
}
