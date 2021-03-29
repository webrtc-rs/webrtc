mod h264_def;
mod h264_test;

pub use h264_def::{H264Packet, H264Payloader};

const STAPA_NALU_TYPE: u8 = 24;
const FUA_NALU_TYPE: u8 = 28;
const FUA_HEADER_SIZE: usize = 2;
const STAPA_HEADER_SIZE: usize = 1;
const STAPA_NALU_LENGTH_SIZE: usize = 2;
const NALU_TYPE_BITMASK: u8 = 0x1F;
const NALU_REF_IDC_BITMASK: u8 = 0x60;
const FUA_START_BITMASK: u8 = 0x80;
const ANNEXB_NALUSTART_CODE: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

fn emit_nalus(nals: &[u8], mut emit: impl FnMut(&[u8])) {
    let next_ind = |nalu: &[u8], start: usize| -> (isize, isize) {
        let mut zero_count = 0;

        for (i, b) in nalu[start..].iter().enumerate() {
            if *b == 0 {
                zero_count += 1;
                continue;
            } else if *b == 1 && zero_count >= 2 {
                return ((start + i - zero_count) as isize, (zero_count + 1) as isize);
            }

            zero_count = 0;
        }

        (-1, -1)
    };

    let (mut next_ind_start, mut next_ind_len) = next_ind(&nals, 0);

    if next_ind_start == -1 {
        emit(&nals);
    } else {
        while next_ind_start != -1 {
            let prev_start = next_ind_start + next_ind_len;
            let (_next_ind_start, _next_ind_len) = next_ind(&nals, prev_start as usize);
            next_ind_start = _next_ind_start;
            next_ind_len = _next_ind_len;

            if next_ind_start != -1 {
                emit(&nals[prev_start as usize..next_ind_start as usize]);
            } else {
                // Emit until end of stream, no end indicator found
                emit(&nals[prev_start as usize..]);
            }
        }
    }
}
