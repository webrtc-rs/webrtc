use crate::packetizer::{Depacketizer, Payloader};

use std::io::Read;

use utils::Error;

#[cfg(test)]
mod g722_test;

#[derive(Debug, Default)]
pub struct G722 {
    payload: Vec<u8>,
}

impl Payloader for G722 {
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

impl Depacketizer for G722 {
    fn depacketize<R: Read>(&mut self, reader: &mut R) -> Result<(), Error> {
        self.payload.clear();
        reader.read_to_end(&mut self.payload)?;
        if self.payload.is_empty() {
            Err(Error::new("Payload is not large enough".to_string()))
        } else {
            Ok(())
        }
    }
}
