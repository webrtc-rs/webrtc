use crate::packetizer::Payloader;
use bytes::BytesMut;

mod g722_test;

pub struct G722Payloader;

impl Payloader for G722Payloader {
    fn payload(&self, mtu: u16, mut payload: &[u8]) -> Vec<Vec<u8>> {
        let mut payloads = vec![];
        if payload.is_empty() || mtu == 0 {
            return payloads;
        }

        while payload.len() > mtu as usize {
            let mut o = vec![0u8; mtu as usize];
            o.copy_from_slice(&payload[..mtu as usize]);
            payload = &payload[mtu as usize..];
            payloads.push(o)
        }

        let mut o = vec![0u8; payload.len()];
        o.copy_from_slice(&payload);
        payloads.push(o);

        payloads
    }
}
