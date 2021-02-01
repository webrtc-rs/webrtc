use crate::{
    errors::RTPError,
    packetizer::{Depacketizer, Payloader},
};
use bytes::BytesMut;

#[cfg(test)]
mod opus_test;

pub struct OpusPayloader;

impl Payloader for OpusPayloader {
    fn payload(&self, _: u16, payload: BytesMut) -> Vec<Vec<u8>> {
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
    fn unmarshal(&mut self, packet: &mut BytesMut) -> Result<BytesMut, RTPError> {
        if packet.is_empty() {
            return Err(RTPError::ShortPacket);
        }

        self.payload = packet.to_owned();
        Ok(packet.to_owned())
    }
}
/// OpusPartitionHeadChecker checks Opus partition head
#[derive(Debug, Default)]
pub struct OpusPartitionHeadChecker {}

impl OpusPartitionHeadChecker {
    // IsPartitionHead checks whether if this is a head of the Opus partition
    pub fn is_partition_head(&mut self, packet: &mut BytesMut) -> bool {
        let mut p = OpusPacket::default();

        if p.unmarshal(packet).is_err() {
            return false;
        }

        true
    }
}
