#[cfg(test)]
mod h264_test;

use crate::{
    error::Error,
    packetizer::{Depacketizer, Payloader},
};

use bytes::{BufMut, Bytes, BytesMut};

/// H264Payloader payloads H264 packets
pub struct H264Payloader;

pub const STAPA_NALU_TYPE: u8 = 24;
pub const FUA_NALU_TYPE: u8 = 28;

pub const FUA_HEADER_SIZE: isize = 2;
pub const STAPA_HEADER_SIZE: usize = 1;
pub const STAPA_NALU_LENGTH_SIZE: usize = 2;

pub const NALU_TYPE_BITMASK: u8 = 0x1F;
pub const NALU_REF_IDC_BITMASK: u8 = 0x60;
pub const FUA_START_BITMASK: u8 = 0x80;

pub static ANNEXB_NALUSTART_CODE: Bytes = Bytes::from_static(&[0x00, 0x00, 0x00, 0x01]);

fn next_ind(nalu: &Bytes, start: usize) -> (isize, isize) {
    let mut zero_count = 0;

    for (i, &b) in nalu[start..].iter().enumerate() {
        if b == 0 {
            zero_count += 1;
            continue;
        } else if b == 1 && zero_count >= 2 {
            return ((start + i - zero_count) as isize, zero_count as isize + 1);
        }
        zero_count = 0
    }
    (-1, -1)
}

fn emit(nalu: &Bytes, mtu: usize, payloads: &mut Vec<Bytes>) {
    let nalu_type = nalu[0] & NALU_TYPE_BITMASK;
    let nalu_ref_idc = nalu[0] & NALU_REF_IDC_BITMASK;

    if nalu_type == 9 || nalu_type == 12 {
        return;
    }

    // Single NALU
    if nalu.len() <= mtu {
        payloads.push(nalu.clone());
        return;
    }

    // FU-A
    let max_fragment_size = mtu as isize - FUA_HEADER_SIZE;

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
        let mut out = BytesMut::with_capacity((FUA_HEADER_SIZE + current_fragment_size) as usize);
        // +---------------+
        // |0|1|2|3|4|5|6|7|
        // +-+-+-+-+-+-+-+-+
        // |F|NRI|  Type   |
        // +---------------+
        let b0 = FUA_NALU_TYPE | nalu_ref_idc;
        out.put_u8(b0);

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
        out.put_u8(b1);

        out.put(
            &nalu_data
                [nalu_data_index as usize..(nalu_data_index + current_fragment_size) as usize],
        );
        payloads.push(out.freeze());

        nalu_data_remaining -= current_fragment_size;
        nalu_data_index += current_fragment_size;
    }
}

impl Payloader for H264Payloader {
    /// Payload fragments a H264 packet across one or more byte arrays
    fn payload(&self, mtu: usize, payload: &Bytes) -> Result<Vec<Bytes>, Error> {
        if payload.is_empty() || mtu == 0 {
            return Ok(vec![]);
        }

        let mut payloads = vec![];

        let (mut next_ind_start, mut next_ind_len) = next_ind(payload, 0);
        if next_ind_start == -1 {
            emit(payload, mtu, &mut payloads);
        } else {
            while next_ind_start != -1 {
                let prev_start = (next_ind_start + next_ind_len) as usize;
                let (next_ind_start2, next_ind_len2) = next_ind(payload, prev_start);
                next_ind_start = next_ind_start2;
                next_ind_len = next_ind_len2;
                if next_ind_start != -1 {
                    emit(
                        &payload.slice(prev_start..next_ind_start as usize),
                        mtu,
                        &mut payloads,
                    );
                } else {
                    // Emit until end of stream, no end indicator found
                    emit(&payload.slice(prev_start..), mtu, &mut payloads);
                }
            }
        }

        Ok(payloads)
    }
}

/// H264Packet represents the H264 header that is stored in the payload of an RTP Packet
#[derive(Debug, Default)]
pub struct H264Packet {
    pub payload: Bytes,
}

impl Depacketizer for H264Packet {
    /// depacketize parses the passed byte slice and stores the result in the H264Packet this method is called upon
    fn depacketize(&mut self, packet: &Bytes) -> Result<(), Error> {
        if packet.len() <= 2 {
            return Err(Error::ErrShortPacket);
        }

        let mut payload = BytesMut::new();

        // NALU Types
        // https://tools.ietf.org/html/rfc6184#section-5.4
        let b0 = packet[0];
        let nalu_type = b0 & NALU_TYPE_BITMASK;
        if nalu_type > 0 && nalu_type < 24 {
            payload.put(&*ANNEXB_NALUSTART_CODE);
            payload.put(&*packet.clone());
            self.payload = payload.freeze();
            Ok(())
        } else if nalu_type == STAPA_NALU_TYPE {
            let mut curr_offset = STAPA_HEADER_SIZE;
            while curr_offset < packet.len() {
                let nalu_size =
                    ((packet[curr_offset] as usize) << 8) | packet[curr_offset + 1] as usize;
                curr_offset += STAPA_NALU_LENGTH_SIZE;

                if packet.len() < curr_offset + nalu_size {
                    return Err(Error::StapASizeLargerThanBuffer(
                        nalu_size,
                        packet.len() - curr_offset,
                    ));
                }
                payload.put(&*ANNEXB_NALUSTART_CODE);
                payload.put(&*packet.slice(curr_offset..curr_offset + nalu_size));
                curr_offset += nalu_size;
            }

            self.payload = payload.freeze();
            Ok(())
        } else if nalu_type == FUA_NALU_TYPE {
            if packet.len() < FUA_HEADER_SIZE as usize {
                return Err(Error::ErrShortPacket);
            }

            let b1 = packet[1];
            if b1 & FUA_START_BITMASK != 0 {
                let nalu_ref_idc = b0 & NALU_REF_IDC_BITMASK;
                let fragmented_nalu_type = b1 & NALU_TYPE_BITMASK;

                payload.put(&*ANNEXB_NALUSTART_CODE);
                payload.put_u8(nalu_ref_idc | fragmented_nalu_type);
                payload.put_slice(&*packet.slice(FUA_HEADER_SIZE as usize..));

                self.payload = payload.freeze();
                Ok(())
            } else {
                self.payload = packet.slice(FUA_HEADER_SIZE as usize..);
                Ok(())
            }
        } else {
            Err(Error::NaluTypeIsNotHandled(nalu_type))
        }
    }
}
