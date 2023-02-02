use crate::chunk::chunk_payload_data::ChunkPayloadData;
use crate::chunk::chunk_selective_ack::GapAckBlock;
use crate::util::*;

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Default, Debug)]
pub(crate) struct PayloadQueue {
    pub(crate) length: Arc<AtomicUsize>,
    pub(crate) chunk_map: HashMap<u32, ChunkPayloadData>,
    pub(crate) sorted: VecDeque<u32>,
    pub(crate) dup_tsn: Vec<u32>,
    pub(crate) n_bytes: usize,
}

impl PayloadQueue {
    pub(crate) fn new(length: Arc<AtomicUsize>) -> Self {
        length.store(0, Ordering::SeqCst);
        PayloadQueue {
            length,
            ..Default::default()
        }
    }

    pub(crate) fn can_push(&self, p: &ChunkPayloadData, cumulative_tsn: u32) -> bool {
        !(self.chunk_map.contains_key(&p.tsn) || sna32lte(p.tsn, cumulative_tsn))
    }

    pub(crate) fn push_no_check(&mut self, p: ChunkPayloadData) {
        let tsn = p.tsn;
        self.n_bytes += p.user_data.len();
        self.chunk_map.insert(tsn, p);
        self.length.fetch_add(1, Ordering::SeqCst);

        if self.sorted.is_empty() || sna32gt(tsn, *self.sorted.back().unwrap()) {
            self.sorted.push_back(tsn);
        } else if sna32lt(tsn, *self.sorted.front().unwrap()) {
            self.sorted.push_front(tsn);
        } else {
            fn compare_tsn(a: u32, b: u32) -> std::cmp::Ordering {
                if sna32lt(a, b) {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            }
            let pos = match self
                .sorted
                .binary_search_by(|element| compare_tsn(*element, tsn))
            {
                Ok(pos) => pos,
                Err(pos) => pos,
            };
            self.sorted.insert(pos, tsn);
        }
    }

    /// push pushes a payload data. If the payload data is already in our queue or
    /// older than our cumulative_tsn marker, it will be recored as duplications,
    /// which can later be retrieved using popDuplicates.
    pub(crate) fn push(&mut self, p: ChunkPayloadData, cumulative_tsn: u32) -> bool {
        let ok = self.chunk_map.contains_key(&p.tsn);
        if ok || sna32lte(p.tsn, cumulative_tsn) {
            // Found the packet, log in dups
            self.dup_tsn.push(p.tsn);
            return false;
        }

        self.push_no_check(p);
        true
    }

    /// pop pops only if the oldest chunk's TSN matches the given TSN.
    pub(crate) fn pop(&mut self, tsn: u32) -> Option<ChunkPayloadData> {
        if Some(&tsn) == self.sorted.front() {
            self.sorted.pop_front();
            if let Some(c) = self.chunk_map.remove(&tsn) {
                self.length.fetch_sub(1, Ordering::SeqCst);
                self.n_bytes -= c.user_data.len();
                return Some(c);
            }
        }

        None
    }

    /// get returns reference to chunkPayloadData with the given TSN value.
    pub(crate) fn get(&self, tsn: u32) -> Option<&ChunkPayloadData> {
        self.chunk_map.get(&tsn)
    }
    pub(crate) fn get_mut(&mut self, tsn: u32) -> Option<&mut ChunkPayloadData> {
        self.chunk_map.get_mut(&tsn)
    }

    /// popDuplicates returns an array of TSN values that were found duplicate.
    pub(crate) fn pop_duplicates(&mut self) -> Vec<u32> {
        self.dup_tsn.drain(..).collect()
    }

    pub(crate) fn get_gap_ack_blocks(&self, cumulative_tsn: u32) -> Vec<GapAckBlock> {
        if self.chunk_map.is_empty() {
            return vec![];
        }

        let mut b = GapAckBlock::default();
        let mut gap_ack_blocks = vec![];
        for (i, tsn) in self.sorted.iter().enumerate() {
            let diff = if *tsn >= cumulative_tsn {
                (*tsn - cumulative_tsn) as u16
            } else {
                0
            };

            if i == 0 {
                b.start = diff;
                b.end = b.start;
            } else if b.end + 1 == diff {
                b.end += 1;
            } else {
                gap_ack_blocks.push(b);

                b.start = diff;
                b.end = diff;
            }
        }

        gap_ack_blocks.push(b);

        gap_ack_blocks
    }

    pub(crate) fn get_gap_ack_blocks_string(&self, cumulative_tsn: u32) -> String {
        let mut s = format!("cumTSN={cumulative_tsn}");
        for b in self.get_gap_ack_blocks(cumulative_tsn) {
            s += format!(",{}-{}", b.start, b.end).as_str();
        }
        s
    }

    pub(crate) fn mark_as_acked(&mut self, tsn: u32) -> usize {
        let n_bytes_acked = if let Some(c) = self.chunk_map.get_mut(&tsn) {
            c.acked = true;
            c.retransmit = false;
            let n = c.user_data.len();
            self.n_bytes -= n;
            c.user_data.clear();
            n
        } else {
            0
        };

        n_bytes_acked
    }

    pub(crate) fn get_last_tsn_received(&self) -> Option<&u32> {
        self.sorted.back()
    }

    pub(crate) fn mark_all_to_retrasmit(&mut self) {
        for c in self.chunk_map.values_mut() {
            if c.acked || c.abandoned() {
                continue;
            }
            c.retransmit = true;
        }
    }

    pub(crate) fn get_num_bytes(&self) -> usize {
        self.n_bytes
    }

    pub(crate) fn len(&self) -> usize {
        assert_eq!(self.chunk_map.len(), self.length.load(Ordering::SeqCst));
        self.chunk_map.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
