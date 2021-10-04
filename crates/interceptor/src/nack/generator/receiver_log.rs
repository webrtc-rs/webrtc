use crate::error::Error;
use crate::nack::UINT16SIZE_HALF;
use anyhow::Result;

#[derive(Default, Debug)]
struct ReceiveLog {
    packets: Vec<u64>,
    size: u16,
    end: u16,
    started: bool,
    last_consecutive: u16,
}

impl ReceiveLog {
    fn new(size: u16) -> Result<Self> {
        let mut correct_size = false;
        for i in 6..16 {
            if size == (1 << i) {
                correct_size = true;
                break;
            }
        }

        if !correct_size {
            return Err(Error::ErrInvalidSize.into());
        }

        Ok(ReceiveLog {
            packets: vec![0u64; (size as usize) / 64],
            size,
            ..Default::default()
        })
    }

    fn add(&mut self, seq: u16) {
        if !self.started {
            self.set_received(seq);
            self.end = seq;
            self.started = true;
            self.last_consecutive = seq;
            return;
        }

        //TODO: u16 subtract overflow?
        let diff = seq - self.end;
        if diff == 0 {
            return;
        } else if diff < UINT16SIZE_HALF {
            // this means a positive diff, in other words seq > end (with counting for rollovers)
            let mut i = self.end + 1;
            while i != seq {
                // clear packets between end and seq (these may contain packets from a "size" ago)
                self.del_received(i);
                i += 1;
            }
            self.end = seq;

            if self.last_consecutive + 1 == seq {
                self.last_consecutive = seq;
            } else if seq - self.last_consecutive > self.size {
                self.last_consecutive = seq - self.size;
                self.fix_last_consecutive(); // there might be valid packets at the beginning of the buffer now
            }
        } else if self.last_consecutive + 1 == seq {
            // negative diff, seq < end (with counting for rollovers)
            self.last_consecutive = seq;
            self.fix_last_consecutive(); // there might be other valid packets after seq
        }

        self.set_received(seq);
    }

    fn get(&self, seq: u16) -> bool {
        //TODO: u16 subtract overflow?
        let diff = self.end - seq;
        if diff >= UINT16SIZE_HALF {
            return false;
        }

        if diff >= self.size {
            return false;
        }

        self.get_received(seq)
    }

    fn missing_seq_numbers(&self, skip_last_n: u16) -> Vec<u16> {
        //TODO: u16 subtract overflow?
        let until = self.end - skip_last_n;
        if until - self.last_consecutive >= UINT16SIZE_HALF {
            // until < s.last_consecutive (counting for rollover)
            return vec![];
        }

        let mut missing_packet_seq_nums = vec![];
        let mut i = self.last_consecutive + 1;
        while i != until + 1 {
            if !self.get_received(i) {
                missing_packet_seq_nums.push(i);
            }
            i += 1;
        }

        missing_packet_seq_nums
    }

    fn set_received(&mut self, seq: u16) {
        let pos = (seq % self.size) as usize;
        self.packets[pos / 64] |= 1u64 << (pos % 64);
    }

    fn del_received(&mut self, seq: u16) {
        let pos = (seq % self.size) as usize;
        self.packets[pos / 64] &= u64::MAX ^ (1u64 << (pos % 64));
    }

    fn get_received(&self, seq: u16) -> bool {
        let pos = (seq % self.size) as usize;
        (self.packets[pos / 64] & (1u64 << (pos % 64))) != 0
    }

    fn fix_last_consecutive(&mut self) {
        let mut i = self.last_consecutive + 1;
        while i != self.end + 1 && self.get_received(i) {
            // find all consecutive packets
            i += 1;
        }
        self.last_consecutive = i - 1;
    }
}
