use crate::packetizer::{Depacketizer, Payloader};

use std::io::Read;

use util::Error;

#[cfg(test)]
mod opus_test;

pub struct OpusPayloader;

impl Payloader for OpusPayloader {
    fn payload<R: Read>(&self, _mtu: isize, reader: &mut R) -> Result<Vec<Vec<u8>>, Error> {
        let mut payload = vec![];
        reader.read_to_end(&mut payload)?;
        if payload.is_empty() {
            Ok(vec![])
        } else {
            Ok(vec![payload])
        }
    }
}

#[derive(Debug, Default)]
pub struct OpusPacket {
    payload: Vec<u8>,
}

impl Depacketizer for OpusPacket {
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
