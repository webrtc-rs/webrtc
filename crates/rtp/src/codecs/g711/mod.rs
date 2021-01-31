use crate::error::Error;
use crate::packetizer::Payloader;

use std::io::Read;

#[cfg(test)]
mod g711_test;

pub struct G711Payloader;

impl Payloader for G711Payloader {
    fn payload<R: Read>(&self, mtu: isize, reader: &mut R) -> Result<Vec<Vec<u8>>, Error> {
        let mut payloads = vec![];
        if mtu <= 0 {
            return Ok(payloads);
        }

        let mut payload_data = vec![];
        reader.read_to_end(&mut payload_data)?;
        let mut payload_data_remaining = payload_data.len() as isize;
        let mut payload_data_index: usize = 0;
        while payload_data_remaining > 0 {
            let current_fragment_size = std::cmp::min(mtu, payload_data_remaining) as usize;
            let mut out = vec![];

            out.extend_from_slice(
                &payload_data[payload_data_index..payload_data_index + current_fragment_size],
            );
            payloads.push(out);

            payload_data_remaining -= current_fragment_size as isize;
            payload_data_index += current_fragment_size;
        }

        Ok(payloads)
    }
}
