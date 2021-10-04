use crate::error::Error;

use crate::nack::UINT16SIZE_HALF;
use anyhow::Result;

#[derive(Default, Debug)]
struct SendBuffer {
    packets: Vec<Option<rtp::packet::Packet>>,
    size: u16,
    last_added: u16,
    started: bool,
}

impl SendBuffer {
    fn new(size: u16) -> Result<Self> {
        let mut correct_size = false;
        for i in 0..16 {
            if size == 1 << i {
                correct_size = true;
                break;
            }
        }

        if !correct_size {
            return Err(Error::ErrInvalidSize.into());
        }

        Ok(SendBuffer {
            packets: vec![None; size as usize],
            size,
            ..Default::default()
        })
    }

    fn add(&mut self, packet: &rtp::packet::Packet) {
        let seq = packet.header.sequence_number;
        if !self.started {
            self.packets[(seq % self.size) as usize] = Some(packet.clone());
            self.last_added = seq;
            self.started = true;
            return;
        }

        //TODO: u16 subtract overflow?
        let diff = seq - self.last_added;
        if diff == 0 {
            return;
        } else if diff < UINT16SIZE_HALF {
            let mut i = self.last_added + 1;
            while i != seq {
                self.packets[(i % self.size) as usize] = None;
                i += 1;
            }
        }

        self.packets[(seq % self.size) as usize] = Some(packet.clone());
        self.last_added = seq;
    }

    fn get(&self, seq: u16) -> Option<&rtp::packet::Packet> {
        //TODO: u16 subtract overflow?
        let diff = self.last_added - seq;
        if diff >= UINT16SIZE_HALF {
            return None;
        }

        if diff >= self.size {
            return None;
        }

        self.packets[(seq % self.size) as usize].as_ref()
    }
}
