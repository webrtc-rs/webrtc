use std::vec;

use byteorder::BigEndian;
use byteorder::ByteOrder;

use crate::{
    errors::RTPError,
    packetizer::{Depacketizer, Payloader},
};

pub struct H264Payloader;

/// Payload fragments a H264 packet across one or more byte arrays
impl Payloader for H264Payloader {
    fn payload(&self, mtu: u16, payload: &[u8]) -> Vec<Vec<u8>> {
        let mut payloads = vec![];

        if payload.is_empty() {
            return payloads;
        }

        super::emit_nalus(payload, |nalu| {
            if nalu.is_empty() {
                return;
            }

            let nalu_type = nalu[0] & super::NALU_TYPE_BITMASK;
            let nalu_ref_idc = nalu[0] & super::NALU_REF_IDC_BITMASK;

            if nalu_type == 9 || nalu_type == 12 {
                return;
            }

            // Single NALU
            if nalu.len() <= mtu as usize {
                let mut out = vec![0u8; nalu.len()];
                out.copy_from_slice(&nalu);
                payloads.push(out);
                return;
            }

            // FU-A
            let max_fragment_size = mtu as isize - super::FUA_HEADER_SIZE as isize;

            // The FU payload consists of fragments of the payload of the fragmented
            // NAL unit so that if the fragmentation unit payloads of consecutive
            // FUs are sequentially concatenated, the payload of the fragmented NAL
            // unit can be reconstructed.  The NAL unit type octet of the fragmented
            // NAL unit is not included as such in the fragmentation unit payload,
            // 	but rather the information of the NAL unit type octet of the
            // fragmented NAL unit is conveyed in the F and NRI fields of the FU
            // indicator octet of the fragmentation unit and in the type field of
            // the FU header.  An FU payload MAY have any number of octets and MAY
            // be empty.

            // According to the RFC, the first octet is skipped due to redundant information
            let mut nalu_data_index = 1;
            let nalu_data_length = nalu.len() - nalu_data_index;
            let mut nalu_data_remaining = nalu_data_length;

            if (max_fragment_size).min(nalu_data_remaining as isize) < 0 {
                return;
            }

            while nalu_data_remaining > 0 {
                let current_fragment_size = (max_fragment_size as usize).min(nalu_data_remaining);
                let mut out = vec![0u8; super::FUA_HEADER_SIZE + current_fragment_size];

                // +---------------+
                // |0|1|2|3|4|5|6|7|
                // +-+-+-+-+-+-+-+-+
                // |F|NRI|  Type   |
                // +---------------+
                out[0] = super::FUA_NALU_TYPE;
                out[0] |= nalu_ref_idc;

                // +---------------+
                // |0|1|2|3|4|5|6|7|
                // +-+-+-+-+-+-+-+-+
                // |S|E|R|  Type   |
                // +---------------+

                out[1] = nalu_type;
                if nalu_data_remaining == nalu_data_length {
                    // Set start bit
                    out[1] |= 1 << 7;
                } else if nalu_data_remaining - current_fragment_size == 0 {
                    // Set end bit
                    out[1] |= 1 << 6;
                }

                out[super::FUA_HEADER_SIZE as usize
                    ..super::FUA_HEADER_SIZE as usize + current_fragment_size]
                    .copy_from_slice(
                        &nalu[nalu_data_index..nalu_data_index + current_fragment_size],
                    );

                payloads.push(out);

                nalu_data_remaining -= current_fragment_size;
                nalu_data_index += current_fragment_size;
            }
        });

        payloads
    }
}

#[derive(Debug, Default)]
pub struct H264Packet {}

impl Depacketizer for H264Packet {
    fn depacketize(&mut self, payload: &[u8]) -> Result<Vec<u8>, RTPError> {
        if payload.len() <= 2 {
            return Err(RTPError::ShortPacket);
        }

        // NALU Types
        // https://tools.ietf.org/html/rfc6184#section-5.4
        let nalu_type = payload[0] & super::NALU_TYPE_BITMASK;

        if nalu_type > 0 && nalu_type < 24 {
            let a = [&super::ANNEXB_NALUSTART_CODE[..], &payload[..]].concat();
            return Ok(a);
        } else if nalu_type == super::STAPA_NALU_TYPE {
            let mut current_offset = super::STAPA_HEADER_SIZE;
            let mut result = vec![];

            while current_offset < payload.len() {
                let nalu_size = BigEndian::read_u16(&payload[current_offset..]);
                current_offset += super::STAPA_NALU_LENGTH_SIZE;

                if payload.len() < current_offset + nalu_size as usize {
                    return Err(RTPError::ShortPacket);
                }

                result.extend_from_slice(&super::ANNEXB_NALUSTART_CODE);
                result.extend_from_slice(
                    &payload[current_offset..current_offset + nalu_size as usize],
                );
                current_offset += nalu_size as usize;
            }

            return Ok(result);
        } else if nalu_type == super::FUA_NALU_TYPE {
            if payload.len() < super::FUA_HEADER_SIZE {
                return Err(RTPError::ShortPacket);
            }

            if payload[1] & super::FUA_START_BITMASK != 0 {
                let nalu_ref_idc = payload[0] & super::NALU_REF_IDC_BITMASK;
                let fragmented_nalu_type = payload[1] & super::NALU_TYPE_BITMASK;

                // Take a copy of payload since we are mutating it.
                let mut payload_copy = payload.to_owned();
                payload_copy[super::FUA_HEADER_SIZE - 1] = nalu_ref_idc | fragmented_nalu_type;

                let a = [
                    &super::ANNEXB_NALUSTART_CODE[..],
                    &payload_copy[super::FUA_HEADER_SIZE - 1..],
                ]
                .concat();

                return Ok(a);
            }

            return Ok(payload[super::FUA_HEADER_SIZE..].to_vec());
        }

        Err(RTPError::UnhandledNALUType(nalu_type))
    }
}
