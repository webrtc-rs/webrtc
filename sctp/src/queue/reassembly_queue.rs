use crate::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::util::*;

use crate::error::{Error, Result};

use std::cmp::Ordering;

fn sort_chunks_by_tsn(c: &mut [ChunkPayloadData]) {
    c.sort_by(|a, b| {
        if sna32lt(a.tsn, b.tsn) {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    });
}

fn sort_chunks_by_ssn(c: &mut [ChunkSet]) {
    c.sort_by(|a, b| {
        if sna16lt(a.ssn, b.ssn) {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    });
}

/// chunkSet is a set of chunks that share the same SSN
#[derive(Debug, Clone)]
pub(crate) struct ChunkSet {
    /// used only with the ordered chunks
    pub(crate) ssn: u16,
    pub(crate) ppi: PayloadProtocolIdentifier,
    pub(crate) chunks: Vec<ChunkPayloadData>,
}

impl ChunkSet {
    pub(crate) fn new(ssn: u16, ppi: PayloadProtocolIdentifier) -> Self {
        ChunkSet {
            ssn,
            ppi,
            chunks: vec![],
        }
    }

    pub(crate) fn push(&mut self, chunk: ChunkPayloadData) -> bool {
        // check if dup
        for c in &self.chunks {
            if c.tsn == chunk.tsn {
                return false;
            }
        }

        // append and sort
        self.chunks.push(chunk);
        sort_chunks_by_tsn(&mut self.chunks);

        // Check if we now have a complete set
        self.is_complete()
    }

    pub(crate) fn is_complete(&self) -> bool {
        // Condition for complete set
        //   0. Has at least one chunk.
        //   1. Begins with beginningFragment set to true
        //   2. Ends with endingFragment set to true
        //   3. TSN monotinically increase by 1 from beginning to end

        // 0.
        let n_chunks = self.chunks.len();
        if n_chunks == 0 {
            return false;
        }

        // 1.
        if !self.chunks[0].beginning_fragment {
            return false;
        }

        // 2.
        if !self.chunks[n_chunks - 1].ending_fragment {
            return false;
        }

        // 3.
        let mut last_tsn = 0u32;
        for (i, c) in self.chunks.iter().enumerate() {
            if i > 0 {
                // Fragments must have contiguous TSN
                // From RFC 4960 Section 3.3.1:
                //   When a user message is fragmented into multiple chunks, the TSNs are
                //   used by the receiver to reassemble the message.  This means that the
                //   TSNs for each fragment of a fragmented user message MUST be strictly
                //   sequential.
                if c.tsn != last_tsn + 1 {
                    // mid or end fragment is missing
                    return false;
                }
            }

            last_tsn = c.tsn;
        }

        true
    }
}

#[derive(Default, Debug)]
pub(crate) struct ReassemblyQueue {
    pub(crate) si: u16,
    pub(crate) next_ssn: u16,
    /// expected SSN for next ordered chunk
    pub(crate) ordered: Vec<ChunkSet>,
    pub(crate) unordered: Vec<ChunkSet>,
    pub(crate) unordered_chunks: Vec<ChunkPayloadData>,
    pub(crate) n_bytes: usize,
}

impl ReassemblyQueue {
    /// From RFC 4960 Sec 6.5:
    ///   The Stream Sequence Number in all the streams MUST start from 0 when
    ///   the association is Established.  Also, when the Stream Sequence
    ///   Number reaches the value 65535 the next Stream Sequence Number MUST
    ///   be set to 0.
    pub(crate) fn new(si: u16) -> Self {
        ReassemblyQueue {
            si,
            next_ssn: 0, // From RFC 4960 Sec 6.5:
            ordered: vec![],
            unordered: vec![],
            unordered_chunks: vec![],
            n_bytes: 0,
        }
    }

    pub(crate) fn push(&mut self, chunk: ChunkPayloadData) -> bool {
        if chunk.stream_identifier != self.si {
            return false;
        }

        if chunk.unordered {
            // First, insert into unordered_chunks array
            //atomic.AddUint64(&r.n_bytes, uint64(len(chunk.userData)))
            self.n_bytes += chunk.user_data.len();
            self.unordered_chunks.push(chunk);
            sort_chunks_by_tsn(&mut self.unordered_chunks);

            // Scan unordered_chunks that are contiguous (in TSN)
            // If found, append the complete set to the unordered array
            if let Some(cset) = self.find_complete_unordered_chunk_set() {
                self.unordered.push(cset);
                return true;
            }

            false
        } else {
            // This is an ordered chunk
            if sna16lt(chunk.stream_sequence_number, self.next_ssn) {
                return false;
            }

            self.n_bytes += chunk.user_data.len();

            // Check if a chunkSet with the SSN already exists
            for s in &mut self.ordered {
                if s.ssn == chunk.stream_sequence_number {
                    return s.push(chunk);
                }
            }

            // If not found, create a new chunkSet
            let mut cset = ChunkSet::new(chunk.stream_sequence_number, chunk.payload_type);
            let unordered = chunk.unordered;
            let ok = cset.push(chunk);
            self.ordered.push(cset);
            if !unordered {
                sort_chunks_by_ssn(&mut self.ordered);
            }

            ok
        }
    }

    pub(crate) fn find_complete_unordered_chunk_set(&mut self) -> Option<ChunkSet> {
        let mut start_idx = -1isize;
        let mut n_chunks = 0usize;
        let mut last_tsn = 0u32;
        let mut found = false;

        for (i, c) in self.unordered_chunks.iter().enumerate() {
            // seek beigining
            if c.beginning_fragment {
                start_idx = i as isize;
                n_chunks = 1;
                last_tsn = c.tsn;

                if c.ending_fragment {
                    found = true;
                    break;
                }
                continue;
            }

            if start_idx < 0 {
                continue;
            }

            // Check if contiguous in TSN
            if c.tsn != last_tsn + 1 {
                start_idx = -1;
                continue;
            }

            last_tsn = c.tsn;
            n_chunks += 1;

            if c.ending_fragment {
                found = true;
                break;
            }
        }

        if !found {
            return None;
        }

        // Extract the range of chunks
        let chunks: Vec<ChunkPayloadData> = self
            .unordered_chunks
            .drain(start_idx as usize..(start_idx as usize) + n_chunks)
            .collect();

        let mut chunk_set = ChunkSet::new(0, chunks[0].payload_type);
        chunk_set.chunks = chunks;

        Some(chunk_set)
    }

    pub(crate) fn is_readable(&self) -> bool {
        // Check unordered first
        if !self.unordered.is_empty() {
            // The chunk sets in r.unordered should all be complete.
            return true;
        }

        // Check ordered sets
        if !self.ordered.is_empty() {
            let cset = &self.ordered[0];
            if cset.is_complete() && sna16lte(cset.ssn, self.next_ssn) {
                return true;
            }
        }
        false
    }

    pub(crate) fn read(&mut self, buf: &mut [u8]) -> Result<(usize, PayloadProtocolIdentifier)> {
        // Check unordered first
        let cset = if !self.unordered.is_empty() {
            self.unordered.remove(0)
        } else if !self.ordered.is_empty() {
            // Now, check ordered
            let cset = &self.ordered[0];
            if !cset.is_complete() {
                return Err(Error::ErrTryAgain);
            }
            if sna16gt(cset.ssn, self.next_ssn) {
                return Err(Error::ErrTryAgain);
            }
            if cset.ssn == self.next_ssn {
                // From RFC 4960 Sec 6.5:
                self.next_ssn = self.next_ssn.wrapping_add(1);
            }
            self.ordered.remove(0)
        } else {
            return Err(Error::ErrTryAgain);
        };

        // Concat all fragments into the buffer
        let mut n_written = 0;
        let mut err = None;
        for c in &cset.chunks {
            let to_copy = c.user_data.len();
            self.subtract_num_bytes(to_copy);
            if err.is_none() {
                let n = std::cmp::min(to_copy, buf.len() - n_written);
                buf[n_written..n_written + n].copy_from_slice(&c.user_data[..n]);
                n_written += n;
                if n < to_copy {
                    err = Some(Error::ErrShortBuffer);
                }
            }
        }

        if let Some(err) = err {
            Err(err)
        } else {
            Ok((n_written, cset.ppi))
        }
    }

    /// Use last_ssn to locate a chunkSet then remove it if the set has
    /// not been complete
    pub(crate) fn forward_tsn_for_ordered(&mut self, last_ssn: u16) {
        let num_bytes = self
            .ordered
            .iter()
            .filter(|s| sna16lte(s.ssn, last_ssn) && !s.is_complete())
            .fold(0, |n, s| {
                n + s.chunks.iter().fold(0, |acc, c| acc + c.user_data.len())
            });
        self.subtract_num_bytes(num_bytes);

        self.ordered
            .retain(|s| !sna16lte(s.ssn, last_ssn) || s.is_complete());

        // Finally, forward next_ssn
        if sna16lte(self.next_ssn, last_ssn) {
            self.next_ssn = last_ssn.wrapping_add(1);
        }
    }

    /// Remove all fragments in the unordered sets that contains chunks
    /// equal to or older than `new_cumulative_tsn`.
    /// We know all sets in the r.unordered are complete ones.
    /// Just remove chunks that are equal to or older than new_cumulative_tsn
    /// from the unordered_chunks
    pub(crate) fn forward_tsn_for_unordered(&mut self, new_cumulative_tsn: u32) {
        let mut last_idx: isize = -1;
        for (i, c) in self.unordered_chunks.iter().enumerate() {
            if sna32gt(c.tsn, new_cumulative_tsn) {
                break;
            }
            last_idx = i as isize;
        }
        if last_idx >= 0 {
            for i in 0..(last_idx + 1) as usize {
                self.subtract_num_bytes(self.unordered_chunks[i].user_data.len());
            }
            self.unordered_chunks.drain(..(last_idx + 1) as usize);
        }
    }

    pub(crate) fn subtract_num_bytes(&mut self, n_bytes: usize) {
        if self.n_bytes >= n_bytes {
            self.n_bytes -= n_bytes;
        } else {
            self.n_bytes = 0;
        }
    }

    pub(crate) fn get_num_bytes(&self) -> usize {
        self.n_bytes
    }
}
