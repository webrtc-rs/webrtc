use crate::error::Error;
use crate::packetizer::{Depacketizer, Payloader};

use std::io::Read;

use byteorder::ReadBytesExt;

#[cfg(test)]
mod h264_test;

mod h264;

const STAPA_NALU_TYPE: u8 = 24;
const FUA_NALU_TYPE: u8 = 28;
const FUA_HEADER_SIZE: usize = 2;
const STAPA_HEADER_SIZE: usize = 1;
const STAPA_NALU_LENGTH_SIZE: usize = 2;
const NALU_TYPE_BITMASK: u8 = 0x1F;
const NALU_REF_IDC_BITMASK: u8 = 0x60;
const FUA_START_BITMASK: u8 = 0x80;
const ANNEXB_NALUSTART_CODE: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

fn emit_nalus(nals: BytesMut, mut emit: impl FnMut(BytesMut)) {
    let next_ind = |nalu: BytesMut, start: usize| -> (isize, isize) {
        let mut zero_count = 0;

        for (i, b) in nalu[start..].iter().enumerate() {
            if *b == 0 {
                zero_count += 1;
                continue;
            } else if *b == 1 {
                if zero_count >= 2 {
                    return ((start + i - zero_count) as isize, (zero_count + 1) as isize);
                }
            }
            zero_count = 0;
        }

        Ok(payloads)
    }
}

#[derive(Debug, Default)]
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
            self.payload.push(b0);
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
                    return Err(Error::StapASizeLargerThanBuffer(
                        nalu_size,
                        payload.len() - curr_offset,
                    ));
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
                // Emit until end of stream, no end indicator found
                emit(nals[prev_start as usize..].into());
            }
        } else {
            Err(Error::NaluTypeIsNotHandled(nalu_type))
        }
    }
}
