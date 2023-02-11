#[cfg(test)]
mod opus_test;

use bytes::Bytes;

use crate::error::{Error, Result};
use crate::packetizer::{Depacketizer, Payloader};

#[derive(Default, Debug, Copy, Clone)]
pub struct OpusPayloader;

impl Payloader for OpusPayloader {
    fn payload(&mut self, mtu: usize, payload: &Bytes) -> Result<Vec<Bytes>> {
        if payload.is_empty() || mtu == 0 {
            return Ok(vec![]);
        }

        Ok(vec![payload.clone()])
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(*self)
    }
}

/// OpusPacket represents the Opus header that is stored in the payload of an RTP Packet
#[derive(PartialEq, Eq, Debug, Default, Clone)]
pub struct OpusPacket;

impl Depacketizer for OpusPacket {
    fn depacketize(&mut self, packet: &Bytes) -> Result<Bytes> {
        if packet.is_empty() {
            Err(Error::ErrShortPacket)
        } else {
            Ok(packet.clone())
        }
    }

    fn is_partition_head(&self, _payload: &Bytes) -> bool {
        true
    }

    fn is_partition_tail(&self, _marker: bool, _payload: &Bytes) -> bool {
        true
    }
}
