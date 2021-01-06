use crate::error::Error;
use crate::packetizer::Payloader;

use std::io::Read;

#[cfg(test)]
mod g711_test;

pub struct G711Payloader;

impl Payloader for G711Payloader {
    fn payload(&self, mtu: usize, mut payload: BytesMut) -> Vec<Vec<u8>> {
        let mut payloads = vec![];
        if payload.is_empty() || mtu == 0 {
            return payloads;
        }

        while payload.len() > mtu {
            let mut o = vec![0u8; mtu];
            o.copy_from_slice(&payload[..mtu]);
            payload = payload.split_off(mtu);
            payloads.push(o)
        }

        let mut o = vec![0u8; payload.len()];
        o.copy_from_slice(&payload);
        payloads.push(o);

        payloads
    }
}
