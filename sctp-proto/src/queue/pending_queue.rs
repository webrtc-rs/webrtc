use crate::chunk::chunk_payload_data::ChunkPayloadData;

use std::collections::VecDeque;

/// pendingBaseQueue
pub(crate) type PendingBaseQueue = VecDeque<ChunkPayloadData>;

/// pendingQueue
#[derive(Debug, Default)]
pub(crate) struct PendingQueue {
    unordered_queue: PendingBaseQueue,
    ordered_queue: PendingBaseQueue,
    queue_len: usize,
    n_bytes: usize,
    selected: bool,
    unordered_is_selected: bool,
}

impl PendingQueue {
    pub(crate) fn new() -> Self {
        PendingQueue::default()
    }

    pub(crate) fn push(&mut self, c: ChunkPayloadData) {
        self.n_bytes += c.user_data.len();
        if c.unordered {
            self.unordered_queue.push_back(c);
        } else {
            self.ordered_queue.push_back(c);
        }
        self.queue_len += 1;
    }

    pub(crate) fn peek(&self) -> Option<&ChunkPayloadData> {
        if self.selected {
            if self.unordered_is_selected {
                return self.unordered_queue.get(0);
            } else {
                return self.ordered_queue.get(0);
            }
        }

        let c = self.unordered_queue.get(0);

        if c.is_some() {
            return c;
        }

        self.ordered_queue.get(0)
    }

    pub(crate) fn pop(
        &mut self,
        beginning_fragment: bool,
        unordered: bool,
    ) -> Option<ChunkPayloadData> {
        let popped = if self.selected {
            let popped = if self.unordered_is_selected {
                self.unordered_queue.pop_front()
            } else {
                self.ordered_queue.pop_front()
            };
            if let Some(p) = &popped {
                if p.ending_fragment {
                    self.selected = false;
                }
            }
            popped
        } else {
            if !beginning_fragment {
                return None;
            }
            if unordered {
                let popped = { self.unordered_queue.pop_front() };
                if let Some(p) = &popped {
                    if !p.ending_fragment {
                        self.selected = true;
                        self.unordered_is_selected = true;
                    }
                }
                popped
            } else {
                let popped = { self.ordered_queue.pop_front() };
                if let Some(p) = &popped {
                    if !p.ending_fragment {
                        self.selected = true;
                        self.unordered_is_selected = false;
                    }
                }
                popped
            }
        };

        if let Some(p) = &popped {
            self.n_bytes -= p.user_data.len();
            self.queue_len -= 1;
        }

        popped
    }

    pub(crate) fn get_num_bytes(&self) -> usize {
        self.n_bytes
    }

    pub(crate) fn len(&self) -> usize {
        self.queue_len
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
