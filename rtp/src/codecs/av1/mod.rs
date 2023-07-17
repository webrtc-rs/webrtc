use std::cmp::min;

use bytes::Bytes;

use crate::packetizer::Payloader;

#[derive(Default, Clone, Debug)]
pub struct Av1Payloader {}

const AV1_PAYLOADER_HEADER_SIZE: usize = 1;

// fn encode_leb128(mut val: u32) -> u32 {
//     let mut b = 0;
//     loop {
//         b |= val & 0b01111111;
//         val >>= 7;
//         if val != 0 {
//             b |= 0x80;
//             b <<= 8;
//         } else {
//             return b;
//         }
//     }
// }

fn encode_leb128(input: u32) -> u32 {
    let mut output: u32 = 0;
    let mut in_value = input;

    loop {
        output |= in_value & 0b0111_1111;
        in_value >>= 7;

        if in_value != 0 {
            output |= 0b1000_0000;
            output <<= 8;
        } else {
            return output;
        }
    }
}

impl Payloader for Av1Payloader {
    fn payload(&mut self, mtu: usize, payload: &Bytes) -> crate::error::Result<Vec<Bytes>> {
        let max_fragment_size = mtu - AV1_PAYLOADER_HEADER_SIZE;
        let mut packets = vec![];
        let mut payload_data_remaining = payload.len();
        let mut payload_data_index = 0;

        if min(max_fragment_size, payload_data_remaining) <= 0 {
            return Ok(packets);
        }

        while payload_data_remaining > 0 {
            let current_fragment_size = min(max_fragment_size, payload_data_remaining);
            let leb128_size = if current_fragment_size >= 127 { 2 } else { 1 };

            let mut packet = vec![0; AV1_PAYLOADER_HEADER_SIZE + leb128_size + current_fragment_size];
            let leb128_value = encode_leb128(current_fragment_size as u32);
            if leb128_size == 1 {
                packet[1] = leb128_value as u8;
            } else {
                packet[1] = (leb128_value >> 8) as u8;
                packet[2] = leb128_value as u8;
            }
            packet[AV1_PAYLOADER_HEADER_SIZE + leb128_size..]
                .copy_from_slice(&payload[payload_data_index..payload_data_index + current_fragment_size]);

            payload_data_remaining -= current_fragment_size;
            payload_data_index += current_fragment_size;

            if packets.len() == 0 {
                // Set the N bit to 1 for the first packet
                packet[0] |= 0b00001000;
            }
            if packets.len() > 0 {
                // Set the Z bit to 0 for all but the first packet
                packet[0] |= 0b10000000;
            }
            if payload_data_remaining != 0 {
                // Set the Y bit to 1 for all but the last packet
                packet[0] |= 0b01000000;
            }

            packets.push(Bytes::from(packet));
        }

        Ok(packets)
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(self.clone())
    }
}
