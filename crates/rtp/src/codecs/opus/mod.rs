use crate::error::Error;
use crate::packetizer::{Depacketizer, Payloader};

use bytes::Bytes;

#[cfg(test)]
mod opus_test;

pub struct OpusPayloader;

impl Payloader for OpusPayloader {
    fn payload(&self, mtu: usize, payload: &Bytes) -> Result<Vec<Bytes>, Error> {
        if payload.is_empty() || mtu == 0 {
            return Ok(vec![]);
        }

        Ok(vec![payload.clone()])
    }
}

/// OpusPacket represents the Opus header that is stored in the payload of an RTP Packet
#[derive(Debug, Default)]
pub struct OpusPacket {
    pub payload: Bytes,
}

impl Depacketizer for OpusPacket {
    fn depacketize(&mut self, packet: &Bytes) -> Result<(), Error> {
        if packet.is_empty() {
            Err(Error::ErrShortPacket)
        } else {
            self.payload = packet.clone();
            Ok(())
        }
    }
}
