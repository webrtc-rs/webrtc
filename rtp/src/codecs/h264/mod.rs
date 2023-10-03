#[cfg(test)]
mod h264_test;

use bytes::{BufMut, Bytes, BytesMut};

use crate::error::{Error, Result};
use crate::packetizer::{Depacketizer, Payloader};

/// H264Payloader payloads H264 packets
#[derive(Default, Debug, Clone)]
pub struct H264Payloader {
    sps_nalu: Option<Bytes>,
    pps_nalu: Option<Bytes>,
}

pub const STAPA_NALU_TYPE: u8 = 24;
pub const FUA_NALU_TYPE: u8 = 28;
pub const FUB_NALU_TYPE: u8 = 29;
pub const SPS_NALU_TYPE: u8 = 7;
pub const PPS_NALU_TYPE: u8 = 8;
pub const AUD_NALU_TYPE: u8 = 9;
pub const FILLER_NALU_TYPE: u8 = 12;

pub const FUA_HEADER_SIZE: usize = 2;
pub const STAPA_HEADER_SIZE: usize = 1;
pub const STAPA_NALU_LENGTH_SIZE: usize = 2;

pub const NALU_TYPE_BITMASK: u8 = 0x1F;
pub const NALU_REF_IDC_BITMASK: u8 = 0x60;
pub const FU_START_BITMASK: u8 = 0x80;
pub const FU_END_BITMASK: u8 = 0x40;

pub const OUTPUT_STAP_AHEADER: u8 = 0x78;

pub static ANNEXB_NALUSTART_CODE: Bytes = Bytes::from_static(&[0x00, 0x00, 0x00, 0x01]);

impl H264Payloader {
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

    fn emit(&mut self, nalu: &Bytes, mtu: usize, payloads: &mut Vec<Bytes>) {
        if nalu.is_empty() {
            return;
        }

        let nalu_type = nalu[0] & NALU_TYPE_BITMASK;
        let nalu_ref_idc = nalu[0] & NALU_REF_IDC_BITMASK;

        if nalu_type == AUD_NALU_TYPE || nalu_type == FILLER_NALU_TYPE {
            return;
        } else if nalu_type == SPS_NALU_TYPE {
            self.sps_nalu = Some(nalu.clone());
            return;
        } else if nalu_type == PPS_NALU_TYPE {
            self.pps_nalu = Some(nalu.clone());
            return;
        } else if let (Some(sps_nalu), Some(pps_nalu)) = (&self.sps_nalu, &self.pps_nalu) {
            // Pack current NALU with SPS and PPS as STAP-A
            let sps_len = (sps_nalu.len() as u16).to_be_bytes();
            let pps_len = (pps_nalu.len() as u16).to_be_bytes();

            let mut stap_a_nalu = Vec::with_capacity(1 + 2 + sps_nalu.len() + 2 + pps_nalu.len());
            stap_a_nalu.push(OUTPUT_STAP_AHEADER);
            stap_a_nalu.extend(sps_len);
            stap_a_nalu.extend_from_slice(sps_nalu);
            stap_a_nalu.extend(pps_len);
            stap_a_nalu.extend_from_slice(pps_nalu);
            if stap_a_nalu.len() <= mtu {
                payloads.push(Bytes::from(stap_a_nalu));
            }
        }

        if self.sps_nalu.is_some() && self.pps_nalu.is_some() {
            self.sps_nalu = None;
            self.pps_nalu = None;
        }

        // Single NALU
        if nalu.len() <= mtu {
            payloads.push(nalu.clone());
            return;
        }

        // FU-A
        let max_fragment_size = mtu as isize - FUA_HEADER_SIZE as isize;

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
            let mut out = BytesMut::with_capacity(FUA_HEADER_SIZE + current_fragment_size as usize);
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
}

impl Payloader for H264Payloader {
    /// Payload fragments a H264 packet across one or more byte arrays
    fn payload(&mut self, mtu: usize, payload: &Bytes) -> Result<Vec<Bytes>> {
        if payload.is_empty() || mtu == 0 {
            return Ok(vec![]);
        }

        let mut payloads = vec![];

        let (mut next_ind_start, mut next_ind_len) = H264Payloader::next_ind(payload, 0);
        if next_ind_start == -1 {
            self.emit(payload, mtu, &mut payloads);
        } else {
            while next_ind_start != -1 {
                let prev_start = (next_ind_start + next_ind_len) as usize;
                let (next_ind_start2, next_ind_len2) = H264Payloader::next_ind(payload, prev_start);
                next_ind_start = next_ind_start2;
                next_ind_len = next_ind_len2;
                if next_ind_start != -1 {
                    self.emit(
                        &payload.slice(prev_start..next_ind_start as usize),
                        mtu,
                        &mut payloads,
                    );
                } else {
                    // Emit until end of stream, no end indicator found
                    self.emit(&payload.slice(prev_start..), mtu, &mut payloads);
                }
            }
        }

        Ok(payloads)
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(self.clone())
    }
}

/// H264Packet represents the H264 header that is stored in the payload of an RTP Packet
#[derive(PartialEq, Eq, Debug, Default, Clone)]
pub struct H264Packet {
    pub is_avc: bool,
    fua_buffer: Option<BytesMut>,
}

impl Depacketizer for H264Packet {
    /// depacketize parses the passed byte slice and stores the result in the H264Packet this method is called upon
    fn depacketize(&mut self, packet: &Bytes) -> Result<Bytes> {
        if packet.len() <= 2 {
            return Err(Error::ErrShortPacket);
        }

        let mut payload = BytesMut::new();

        // NALU Types
        // https://tools.ietf.org/html/rfc6184#section-5.4
        let b0 = packet[0];
        let nalu_type = b0 & NALU_TYPE_BITMASK;

        match nalu_type {
            1..=23 => {
                if self.is_avc {
                    payload.put_u32(packet.len() as u32);
                } else {
                    payload.put(&*ANNEXB_NALUSTART_CODE);
                }
                payload.put(&*packet.clone());
                Ok(payload.freeze())
            }
            STAPA_NALU_TYPE => {
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

                    if self.is_avc {
                        payload.put_u32(nalu_size as u32);
                    } else {
                        payload.put(&*ANNEXB_NALUSTART_CODE);
                    }
                    payload.put(&*packet.slice(curr_offset..curr_offset + nalu_size));
                    curr_offset += nalu_size;
                }

                Ok(payload.freeze())
            }
            FUA_NALU_TYPE => {
                if packet.len() < FUA_HEADER_SIZE {
                    return Err(Error::ErrShortPacket);
                }

                if self.fua_buffer.is_none() {
                    self.fua_buffer = Some(BytesMut::new());
                }

                if let Some(fua_buffer) = &mut self.fua_buffer {
                    fua_buffer.put(&*packet.slice(FUA_HEADER_SIZE..));
                }

                let b1 = packet[1];
                if b1 & FU_END_BITMASK != 0 {
                    let nalu_ref_idc = b0 & NALU_REF_IDC_BITMASK;
                    let fragmented_nalu_type = b1 & NALU_TYPE_BITMASK;

                    if let Some(fua_buffer) = self.fua_buffer.take() {
                        if self.is_avc {
                            payload.put_u32((fua_buffer.len() + 1) as u32);
                        } else {
                            payload.put(&*ANNEXB_NALUSTART_CODE);
                        }
                        payload.put_u8(nalu_ref_idc | fragmented_nalu_type);
                        payload.put(fua_buffer);
                    }

                    Ok(payload.freeze())
                } else {
                    Ok(Bytes::new())
                }
            }
            _ => Err(Error::NaluTypeIsNotHandled(nalu_type)),
        }
    }

    /// is_partition_head checks if this is the head of a packetized nalu stream.
    fn is_partition_head(&self, payload: &Bytes) -> bool {
        if payload.len() < 2 {
            return false;
        }

        if payload[0] & NALU_TYPE_BITMASK == FUA_NALU_TYPE
            || payload[0] & NALU_TYPE_BITMASK == FUB_NALU_TYPE
        {
            (payload[1] & FU_START_BITMASK) != 0
        } else {
            true
        }
    }

    fn is_partition_tail(&self, marker: bool, _payload: &Bytes) -> bool {
        marker
    }
}
