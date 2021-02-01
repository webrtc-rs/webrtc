use crate::packetizer::Payloader;

use bytes::BytesMut;

#[cfg(test)]
mod g711_test;

pub struct G711Payloader;

impl Payloader for G711Payloader {
    fn payload(&self, mtu: u16, mut payload: BytesMut) -> Vec<Vec<u8>> {
        let mut payloads = vec![];
        if payload.is_empty() || mtu == 0 {
            return payloads;
        }

        while payload.len() > mtu as usize {
            let mut o = vec![0u8; mtu as usize];
            o.copy_from_slice(&payload[..mtu as usize]);
            payload = payload.split_off(mtu as usize);
            payloads.push(o)
        }

        let mut o = vec![0u8; payload.len()];
        o.copy_from_slice(&payload);
        payloads.push(o);

        payloads
    }
}
