use crate::packetizer::{Depacketizer, Payloader};

use std::io::Read;

use utils::Error;

#[cfg(test)]
mod opus_test;

struct Opus {
    payload: Vec<u8>,
}

impl Payloader for Opus {
    fn payload<R: Read>(&self, mtu: isize, reader: &mut R) -> Result<Vec<Vec<u8>>, Error> {
        let mut payload = vec![];
        reader.read_to_end(&mut payload)?;
        if payload.is_empty() {
            Ok(vec![])
        } else {
            Ok(vec![payload])
        }
    }
}

impl Depacketizer for Opus {
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
