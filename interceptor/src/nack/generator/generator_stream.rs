use super::*;

use crate::nack::UINT16SIZE_HALF;

use util::sync::Mutex;
use util::Unmarshal;

struct GeneratorStreamInternal {
    packets: Vec<u64>,
    size: u16,
    end: u16,
    started: bool,
    last_consecutive: u16,
}

impl GeneratorStreamInternal {
    fn new(log2_size_minus_6: u8) -> Self {
        GeneratorStreamInternal {
            packets: vec![0u64; 1 << log2_size_minus_6],
            size: 1 << (log2_size_minus_6 + 6),
            end: 0,
            started: false,
            last_consecutive: 0,
        }
    }

    fn add(&mut self, seq: u16) {
        if !self.started {
            self.set_received(seq);
            self.end = seq;
            self.started = true;
            self.last_consecutive = seq;
            return;
        }

        let last_consecutive_plus1 = self.last_consecutive.wrapping_add(1);
        let diff = seq.wrapping_sub(self.end);
        if diff == 0 {
            return;
        } else if diff < UINT16SIZE_HALF {
            // this means a positive diff, in other words seq > end (with counting for rollovers)
            let mut i = self.end.wrapping_add(1);
            while i != seq {
                // clear packets between end and seq (these may contain packets from a "size" ago)
                self.del_received(i);
                i = i.wrapping_add(1);
            }
            self.end = seq;

            let seq_sub_last_consecutive = seq.wrapping_sub(self.last_consecutive);
            if last_consecutive_plus1 == seq {
                self.last_consecutive = seq;
            } else if seq_sub_last_consecutive > self.size {
                let diff = seq.wrapping_sub(self.size);
                self.last_consecutive = diff;
                self.fix_last_consecutive(); // there might be valid packets at the beginning of the buffer now
            }
        } else if last_consecutive_plus1 == seq {
            // negative diff, seq < end (with counting for rollovers)
            self.last_consecutive = seq;
            self.fix_last_consecutive(); // there might be other valid packets after seq
        }

        self.set_received(seq);
    }

    fn get(&self, seq: u16) -> bool {
        let diff = self.end.wrapping_sub(seq);
        if diff >= UINT16SIZE_HALF {
            return false;
        }

        if diff >= self.size {
            return false;
        }

        self.get_received(seq)
    }

    fn missing_seq_numbers(&self, skip_last_n: u16) -> Vec<u16> {
        let until = self.end.wrapping_sub(skip_last_n);
        let diff = until.wrapping_sub(self.last_consecutive);
        if diff >= UINT16SIZE_HALF {
            // until < s.last_consecutive (counting for rollover)
            return vec![];
        }

        let mut missing_packet_seq_nums = vec![];
        let mut i = self.last_consecutive.wrapping_add(1);
        let util_plus1 = until.wrapping_add(1);
        while i != util_plus1 {
            if !self.get_received(i) {
                missing_packet_seq_nums.push(i);
            }
            i = i.wrapping_add(1);
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
        let mut i = self.last_consecutive.wrapping_add(1);
        while i != self.end.wrapping_add(1) && self.get_received(i) {
            // find all consecutive packets
            i = i.wrapping_add(1);
        }
        self.last_consecutive = i.wrapping_sub(1);
    }
}

pub(super) struct GeneratorStream {
    parent_rtp_reader: Arc<dyn RTPReader + Send + Sync>,

    internal: Mutex<GeneratorStreamInternal>,
}

impl GeneratorStream {
    pub(super) fn new(log2_size_minus_6: u8, reader: Arc<dyn RTPReader + Send + Sync>) -> Self {
        GeneratorStream {
            parent_rtp_reader: reader,
            internal: Mutex::new(GeneratorStreamInternal::new(log2_size_minus_6)),
        }
    }

    pub(super) fn missing_seq_numbers(&self, skip_last_n: u16) -> Vec<u16> {
        let internal = self.internal.lock();
        internal.missing_seq_numbers(skip_last_n)
    }

    pub(super) fn add(&self, seq: u16) {
        let mut internal = self.internal.lock();
        internal.add(seq);
    }
}

/// RTPReader is used by Interceptor.bind_remote_stream.
#[async_trait]
impl RTPReader for GeneratorStream {
    /// read a rtp packet
    async fn read(&self, buf: &mut [u8], a: &Attributes) -> Result<(usize, Attributes)> {
        let (n, attr) = self.parent_rtp_reader.read(buf, a).await?;

        let mut b = &buf[..n];
        let pkt = rtp::packet::Packet::unmarshal(&mut b)?;
        self.add(pkt.header.sequence_number);

        Ok((n, attr))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_generator_stream() -> Result<()> {
        let tests: Vec<u16> = vec![
            0, 1, 127, 128, 129, 511, 512, 513, 32767, 32768, 32769, 65407, 65408, 65409, 65534,
            65535,
        ];
        for start in tests {
            let mut rl = GeneratorStreamInternal::new(1);

            let all = |min: u16, max: u16| -> Vec<u16> {
                let mut result = vec![];
                let mut i = min;
                let max_plus_1 = max.wrapping_add(1);
                while i != max_plus_1 {
                    result.push(i);
                    i = i.wrapping_add(1);
                }
                result
            };

            let join = |parts: &[&[u16]]| -> Vec<u16> {
                let mut result = vec![];
                for p in parts {
                    result.extend_from_slice(p);
                }
                result
            };

            let add = |rl: &mut GeneratorStreamInternal, nums: &[u16]| {
                for n in nums {
                    let seq = start.wrapping_add(*n);
                    rl.add(seq);
                }
            };

            let assert_get = |rl: &GeneratorStreamInternal, nums: &[u16]| {
                for n in nums {
                    let seq = start.wrapping_add(*n);
                    assert!(rl.get(seq), "not found: {seq}");
                }
            };

            let assert_not_get = |rl: &GeneratorStreamInternal, nums: &[u16]| {
                for n in nums {
                    let seq = start.wrapping_add(*n);
                    assert!(
                        !rl.get(seq),
                        "packet found: start {}, n {}, seq {}",
                        start,
                        *n,
                        seq
                    );
                }
            };

            let assert_missing = |rl: &GeneratorStreamInternal, skip_last_n: u16, nums: &[u16]| {
                let missing = rl.missing_seq_numbers(skip_last_n);
                let mut want = vec![];
                for n in nums {
                    let seq = start.wrapping_add(*n);
                    want.push(seq);
                }
                assert_eq!(want, missing, "missing want/got, ");
            };

            let assert_last_consecutive = |rl: &GeneratorStreamInternal, last_consecutive: u16| {
                let want = last_consecutive.wrapping_add(start);
                assert_eq!(rl.last_consecutive, want, "invalid last_consecutive want");
            };

            add(&mut rl, &[0]);
            assert_get(&rl, &[0]);
            assert_missing(&rl, 0, &[]);
            assert_last_consecutive(&rl, 0); // first element added

            add(&mut rl, &all(1, 127));
            assert_get(&rl, &all(1, 127));
            assert_missing(&rl, 0, &[]);
            assert_last_consecutive(&rl, 127);

            add(&mut rl, &[128]);
            assert_get(&rl, &[128]);
            assert_not_get(&rl, &[0]);
            assert_missing(&rl, 0, &[]);
            assert_last_consecutive(&rl, 128);

            add(&mut rl, &[130]);
            assert_get(&rl, &[130]);
            assert_not_get(&rl, &[1, 2, 129]);
            assert_missing(&rl, 0, &[129]);
            assert_last_consecutive(&rl, 128);

            add(&mut rl, &[333]);
            assert_get(&rl, &[333]);
            assert_not_get(&rl, &all(0, 332));
            assert_missing(&rl, 0, &all(206, 332)); // all 127 elements missing before 333
            assert_missing(&rl, 10, &all(206, 323)); // skip last 10 packets (324-333) from check
            assert_last_consecutive(&rl, 205); // lastConsecutive is still out of the buffer

            add(&mut rl, &[329]);
            assert_get(&rl, &[329]);
            assert_missing(&rl, 0, &join(&[&all(206, 328), &all(330, 332)]));
            assert_missing(&rl, 5, &join(&[&all(206, 328)])); // skip last 5 packets (329-333) from check
            assert_last_consecutive(&rl, 205);

            add(&mut rl, &all(207, 320));
            assert_get(&rl, &all(207, 320));
            assert_missing(&rl, 0, &join(&[&[206], &all(321, 328), &all(330, 332)]));
            assert_last_consecutive(&rl, 205);

            add(&mut rl, &[334]);
            assert_get(&rl, &[334]);
            assert_not_get(&rl, &[206]);
            assert_missing(&rl, 0, &join(&[&all(321, 328), &all(330, 332)]));
            assert_last_consecutive(&rl, 320); // head of buffer is full of consecutive packages

            add(&mut rl, &all(322, 328));
            assert_get(&rl, &all(322, 328));
            assert_missing(&rl, 0, &join(&[&[321], &all(330, 332)]));
            assert_last_consecutive(&rl, 320);

            add(&mut rl, &[321]);
            assert_get(&rl, &[321]);
            assert_missing(&rl, 0, &all(330, 332));
            assert_last_consecutive(&rl, 329); // after adding a single missing packet, lastConsecutive should jump forward
        }

        Ok(())
    }

    #[test]
    fn test_generator_stream_rollover() {
        let mut rl = GeneratorStreamInternal::new(1);
        // Make sure it doesn't panic.
        rl.add(65533);
        rl.add(65535);
        rl.add(65534);

        let mut rl = GeneratorStreamInternal::new(1);
        // Make sure it doesn't panic.
        rl.add(65534);
        rl.add(0);
        rl.add(65535);
    }
}
