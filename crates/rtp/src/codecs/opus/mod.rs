#[cfg(test)]
mod opus_test;

use crate::{
    error::Error,
    packetizer::{Depacketizer, Payloader},
};

use anyhow::Result;
use bytes::Bytes;

pub struct OpusPayloader;

impl Payloader for OpusPayloader {
    fn payload(&self, mtu: usize, payload: &Bytes) -> Result<Vec<Bytes>> {
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
    fn depacketize(&mut self, packet: &Bytes) -> Result<()> {
        if packet.is_empty() {
            Err(Error::ErrShortPacket.into())
        } else {
            self.payload = packet.clone();
            Ok(())
        }
    }
}
