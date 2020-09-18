use crate::packetizer::{Depacketizer, Payloader};

use std::io::Read;

use byteorder::ReadBytesExt;
use util::Error;

#[cfg(test)]
mod h264_test;

pub struct H264Payloader;

const STAPA_NALU_TYPE: u8 = 24;
const FUA_NALU_TYPE: u8 = 28;

const FUA_HEADER_SIZE: isize = 2;
const STAPA_HEADER_SIZE: usize = 1;
const STAPA_NALU_LENGTH_SIZE: usize = 2;

const NALU_TYPE_BITMASK: u8 = 0x1F;
const NALU_REF_IDC_BITMASK: u8 = 0x60;
const FUA_START_BITMASK: u8 = 0x80;

static ANNEXB_NALUSTART_CODE: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

fn next_ind(nalu: &[u8], start: usize) -> (isize, isize) {
    let mut zero_count = 0;

    for (i, &b) in nalu[start..].iter().enumerate() {
        if b == 0 {
            zero_count += 1;
            continue;
        } else if b == 1 {
            if zero_count >= 2 {
                return ((start + i - zero_count) as isize, zero_count as isize + 1);
            }
        }
        zero_count = 0
    }
    (-1, -1)
}

fn emit(nalu: &[u8], mtu: isize, payloads: &mut Vec<Vec<u8>>) {
    let nalu_type = nalu[0] & NALU_TYPE_BITMASK;
    let nalu_ref_idc = nalu[0] & NALU_REF_IDC_BITMASK;

    if nalu_type == 9 || nalu_type == 12 {
        return;
    }

    // Single NALU
    if nalu.len() as isize <= mtu {
        let mut out = vec![];

        out.extend_from_slice(nalu);
        payloads.push(out);

        return;
    }

    // FU-A
    let max_fragment_size = mtu - FUA_HEADER_SIZE;

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

    let nalu_data = nalu;
    // According to the RFC, the first octet is skipped due to redundant information
    let mut nalu_data_index = 1;
    let nalu_data_length = nalu.len() as isize - nalu_data_index;
    let mut nalu_data_remaining = nalu_data_length;

    if std::cmp::min(max_fragment_size, nalu_data_remaining) <= 0 {
        return;
    }

    while nalu_data_remaining > 0 {
        let current_fragment_size = std::cmp::min(max_fragment_size, nalu_data_remaining);
        //out: = make([]byte, fuaHeaderSize + currentFragmentSize)
        let mut out = vec![];
        // +---------------+
        // |0|1|2|3|4|5|6|7|
        // +-+-+-+-+-+-+-+-+
        // |F|NRI|  Type   |
        // +---------------+
        let b0 = FUA_NALU_TYPE | nalu_ref_idc;
        out.push(b0);

        // +---------------+
        //|0|1|2|3|4|5|6|7|
        //+-+-+-+-+-+-+-+-+
        //|S|E|R|  Type   |
        //+---------------+

        let mut b1 = nalu_type;
        if nalu_data_remaining == nalu_data_length {
            // Set start bit
            b1 |= 1 << 7;
        } else if nalu_data_remaining - current_fragment_size == 0 {
            // Set end bit
            b1 |= 1 << 6;
        }
        out.push(b1);

        out.extend_from_slice(
            &nalu_data
                [nalu_data_index as usize..(nalu_data_index + current_fragment_size) as usize],
        );
        payloads.push(out);

        nalu_data_remaining -= current_fragment_size;
        nalu_data_index += current_fragment_size;
    }
}

// Payload fragments a H264 packet across one or more byte arrays
impl Payloader for H264Payloader {
    fn payload<R: Read>(&self, mtu: isize, reader: &mut R) -> Result<Vec<Vec<u8>>, Error> {
        let mut payloads = vec![];

        let mut nals = vec![];
        reader.read_to_end(&mut nals)?;
        if nals.is_empty() {
            return Ok(payloads);
        }

        let (mut next_ind_start, mut next_ind_len) = next_ind(&nals, 0);
        if next_ind_start == -1 {
            emit(&nals, mtu, &mut payloads);
        } else {
            while next_ind_start != -1 {
                let prev_start = (next_ind_start + next_ind_len) as usize;
                let (next_ind_start2, next_ind_len2) = next_ind(&nals, prev_start);
                next_ind_start = next_ind_start2;
                next_ind_len = next_ind_len2;
                if next_ind_start != -1 {
                    emit(
                        &nals[prev_start..next_ind_start as usize],
                        mtu,
                        &mut payloads,
                    );
                } else {
                    // Emit until end of stream, no end indicator found
                    emit(&nals[prev_start..], mtu, &mut payloads);
                }
            }
        }

        Ok(payloads)
    }
}

pub struct H264Packet {
    payload: Vec<u8>,
}

impl Depacketizer for H264Packet {
    fn depacketize<R: Read>(&mut self, reader: &mut R) -> Result<(), Error> {
        self.payload.clear();

        // NALU Types
        // https://tools.ietf.org/html/rfc6184#section-5.4
        let b0 = reader.read_u8()?;
        let nalu_type = b0 & NALU_TYPE_BITMASK;
        if nalu_type > 0 && nalu_type < 24 {
            self.payload.append(&mut ANNEXB_NALUSTART_CODE.to_vec());
            reader.read_to_end(&mut self.payload)?;
            Ok(())
        } else if nalu_type == STAPA_NALU_TYPE {
            let mut curr_offset = 0;
            let mut payload = vec![];
            reader.read_to_end(&mut payload)?;

            while curr_offset + 1 < payload.len() {
                let nalu_size =
                    ((payload[curr_offset] as usize) << 8) | payload[curr_offset + 1] as usize;
                curr_offset += STAPA_NALU_LENGTH_SIZE;

                if curr_offset + nalu_size > payload.len() {
                    return Err(Error::new(format!(
                        "STAP-A declared size({}) is larger than buffer({})",
                        nalu_size,
                        payload.len() - curr_offset
                    )));
                }
                self.payload.append(&mut ANNEXB_NALUSTART_CODE.to_vec());
                self.payload
                    .append(&mut payload[curr_offset..curr_offset + nalu_size].to_vec());
                curr_offset += nalu_size;
            }

            Ok(())
        } else if nalu_type == FUA_NALU_TYPE {
            let b1 = reader.read_u8()?;
            if b1 & FUA_START_BITMASK != 0 {
                let nalu_ref_idc = b0 & NALU_REF_IDC_BITMASK;
                let fragmented_nalu_type = b1 & NALU_TYPE_BITMASK;

                self.payload.append(&mut ANNEXB_NALUSTART_CODE.to_vec());
                self.payload.push(nalu_ref_idc | fragmented_nalu_type);
                reader.read_to_end(&mut self.payload)?;

                Ok(())
            } else {
                reader.read_to_end(&mut self.payload)?;
                Ok(())
            }
        } else {
            Err(Error::new(format!(
                "nalu type {} is currently not handled",
                nalu_type
            )))
        }
    }
}
