use crate::error::Error;
use crate::packetizer::{Depacketizer, Payloader};

use std::io::Read;

#[cfg(test)]
mod opus_test;

pub struct OpusPayloader;

impl Payloader for OpusPayloader {
    fn payload(&self, _: usize, payload: BytesMut) -> Vec<Vec<u8>> {
        if payload.is_empty() {
            return vec![];
        }

        let mut out = vec![0u8; payload.len()];
        out.copy_from_slice(&payload);

        vec![out]
    }
}

#[derive(Debug, Default)]
pub struct OpusPacket {
    payload: BytesMut,
}

impl Depacketizer for OpusPacket {
    fn depacketize<R: Read>(&mut self, reader: &mut R) -> Result<(), Error> {
        self.payload.clear();
        reader.read_to_end(&mut self.payload)?;
        if self.payload.is_empty() {
            Err(Error::PayloadIsNotLargeEnough)
        } else {
            Ok(())
        }

        true
    }
}
