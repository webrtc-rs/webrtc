use crate::nack::UINT16SIZE_HALF;

#[derive(Default, Debug)]
struct ResponderStream {
    packets: Vec<Option<rtp::packet::Packet>>,
    size: u16,
    last_added: u16,
    started: bool,
}

impl ResponderStream {
    fn new(log2_size: u8) -> Self {
        ResponderStream {
            packets: vec![None; 1 << log2_size],
            size: 1 << log2_size,
            ..Default::default()
        }
    }

    fn add(&mut self, packet: &rtp::packet::Packet) {
        let seq = packet.header.sequence_number;
        if !self.started {
            self.packets[(seq % self.size) as usize] = Some(packet.clone());
            self.last_added = seq;
            self.started = true;
            return;
        }

        let (diff, _) = seq.overflowing_sub(self.last_added);
        if diff == 0 {
            return;
        } else if diff < UINT16SIZE_HALF {
            let (mut i, _) = self.last_added.overflowing_add(1);
            while i != seq {
                self.packets[(i % self.size) as usize] = None;
                let (j, _) = i.overflowing_add(1);
                i = j;
            }
        }

        self.packets[(seq % self.size) as usize] = Some(packet.clone());
        self.last_added = seq;
    }

    fn get(&self, seq: u16) -> Option<&rtp::packet::Packet> {
        let (diff, _) = self.last_added.overflowing_sub(seq);
        if diff >= UINT16SIZE_HALF {
            return None;
        }

        if diff >= self.size {
            return None;
        }

        self.packets[(seq % self.size) as usize].as_ref()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_responder_stream() -> Result<()> {
        let tests: Vec<u16> = vec![
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 511, 512, 513, 32767, 32768, 32769, 65527, 65528, 65529,
            65530, 65531, 65532, 65533, 65534, 65535,
        ];
        for start in tests {
            let mut sb = ResponderStream::new(3);

            let add = |sb: &mut ResponderStream, nums: &[u16]| {
                for n in nums {
                    let (seq, _) = start.overflowing_add(*n);
                    sb.add(&rtp::packet::Packet {
                        header: rtp::header::Header {
                            sequence_number: seq,
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                }
            };

            let assert_get = |sb: &ResponderStream, nums: &[u16]| {
                for n in nums {
                    let (seq, _) = start.overflowing_add(*n);
                    if let Some(packet) = sb.get(seq) {
                        assert_eq!(
                            packet.header.sequence_number, seq,
                            "packet for {} returned with incorrect SequenceNumber: {}",
                            seq, packet.header.sequence_number
                        );
                    } else {
                        assert!(false, "packet not found: {}", seq);
                    }
                }
            };

            let assert_not_get = |sb: &ResponderStream, nums: &[u16]| {
                for n in nums {
                    let (seq, _) = start.overflowing_add(*n);
                    if let Some(packet) = sb.get(seq) {
                        assert!(
                            false,
                            "packet found for {}: {}",
                            seq, packet.header.sequence_number
                        );
                    }
                }
            };

            add(&mut sb, &[0, 1, 2, 3, 4, 5, 6, 7]);
            assert_get(&sb, &[0, 1, 2, 3, 4, 5, 6, 7]);

            add(&mut sb, &[8]);
            assert_get(&sb, &[8]);
            assert_not_get(&sb, &[0]);

            add(&mut sb, &[10]);
            assert_get(&sb, &[10]);
            assert_not_get(&sb, &[1, 2, 9]);

            add(&mut sb, &[22]);
            assert_get(&sb, &[22]);
            assert_not_get(
                &sb,
                &[
                    3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
                ],
            );
        }

        Ok(())
    }
}
