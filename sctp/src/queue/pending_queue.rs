use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Condvar, Mutex,
    },
};

use util::sync::RwLock;

use crate::chunk::chunk_payload_data::ChunkPayloadData;

/// Basic queue for either ordered or unordered chunks.
pub(crate) type PendingBaseQueue = VecDeque<ChunkPayloadData>;

// TODO: benchmark performance between multiple Atomic+Mutex vs one Mutex<PendingQueueInternal>

/// A queue for both ordered and unordered chunks.
#[derive(Debug, Default)]
pub(crate) struct PendingQueue {
    semaphore: PushLimitSemaphore,
    unordered_queue: RwLock<PendingBaseQueue>,
    ordered_queue: RwLock<PendingBaseQueue>,
    queue_len: AtomicUsize,
    n_bytes: AtomicUsize,
    selected: AtomicBool,
    unordered_is_selected: AtomicBool,
}

/// Simple diy semaphore not directly protecting a resource other than the current capacity of the Queue in bytes
#[derive(Debug)]
struct PushLimitSemaphore {
    m: Mutex<u64>,
    c: Condvar,
}

impl Default for PushLimitSemaphore {
    fn default() -> Self {
        Self {
            m: Mutex::new(1_000_000_000),
            c: Condvar::new(),
        }
    }
}

impl PushLimitSemaphore {
    /// blocks until the credits where sucessfully taken
    fn aquire(&self, credits: u64) {
        let capacity = self.m.lock().unwrap();
        let mut capacity = self
            .c
            .wait_while(capacity, |capacity| *capacity < credits)
            .unwrap();
        assert!(*capacity >= credits);
        *capacity = *capacity - credits;
    }

    /// releases credits and allows them to be taken by a process calling aquire
    fn release(&self, credits: u64) {
        let mut capacity = self.m.lock().unwrap();
        *capacity = *capacity + credits;
        self.c.notify_one();
    }
}

impl PendingQueue {
    pub(crate) fn new() -> Self {
        PendingQueue::default()
    }

    /// Appends a chunk to the back of the pending queue.
    pub(crate) fn push(&self, c: ChunkPayloadData) {
        let user_data_len = c.user_data.len();

        self.semaphore.aquire(user_data_len as u64);

        if c.unordered {
            let mut unordered_queue = self.unordered_queue.write();
            unordered_queue.push_back(c);
        } else {
            let mut ordered_queue = self.ordered_queue.write();
            ordered_queue.push_back(c);
        }

        self.n_bytes.fetch_add(user_data_len, Ordering::SeqCst);
        self.queue_len.fetch_add(1, Ordering::SeqCst);
    }

    /// Appends chunks to the back of the pending queue.
    ///
    /// # Panics
    ///
    /// If it's a mix of unordered and ordered chunks.
    pub(crate) fn append(&self, chunks: Vec<ChunkPayloadData>) {
        if chunks.is_empty() {
            return;
        }

        let total_user_data_len = chunks.iter().fold(0, |acc, c| acc + c.user_data.len());
        let chunks_len = chunks.len();

        self.semaphore.aquire(total_user_data_len as u64);

        let unordered = chunks
            .first()
            .expect("chunks to not be empty because of the above check")
            .unordered;
        if unordered {
            let mut unordered_queue = self.unordered_queue.write();
            assert!(
                chunks.iter().all(|c| c.unordered),
                "expected all chunks to be unordered"
            );
            unordered_queue.extend(chunks);
        } else {
            let mut ordered_queue = self.ordered_queue.write();
            assert!(
                chunks.iter().all(|c| !c.unordered),
                "expected all chunks to be ordered"
            );
            ordered_queue.extend(chunks);
        }

        self.n_bytes
            .fetch_add(total_user_data_len, Ordering::SeqCst);
        self.queue_len.fetch_add(chunks_len, Ordering::SeqCst);
    }

    pub(crate) fn peek(&self) -> Option<ChunkPayloadData> {
        if self.selected.load(Ordering::SeqCst) {
            if self.unordered_is_selected.load(Ordering::SeqCst) {
                let unordered_queue = self.unordered_queue.read();
                return unordered_queue.get(0).cloned();
            } else {
                let ordered_queue = self.ordered_queue.read();
                return ordered_queue.get(0).cloned();
            }
        }

        let c = {
            let unordered_queue = self.unordered_queue.read();
            unordered_queue.get(0).cloned()
        };

        if c.is_some() {
            return c;
        }

        let ordered_queue = self.ordered_queue.read();
        ordered_queue.get(0).cloned()
    }

    pub(crate) fn pop(
        &self,
        beginning_fragment: bool,
        unordered: bool,
    ) -> Option<ChunkPayloadData> {
        let popped = if self.selected.load(Ordering::SeqCst) {
            let popped = if self.unordered_is_selected.load(Ordering::SeqCst) {
                let mut unordered_queue = self.unordered_queue.write();
                unordered_queue.pop_front()
            } else {
                let mut ordered_queue = self.ordered_queue.write();
                ordered_queue.pop_front()
            };
            if let Some(p) = &popped {
                if p.ending_fragment {
                    self.selected.store(false, Ordering::SeqCst);
                }
            }
            popped
        } else {
            if !beginning_fragment {
                return None;
            }
            if unordered {
                let popped = {
                    let mut unordered_queue = self.unordered_queue.write();
                    unordered_queue.pop_front()
                };
                if let Some(p) = &popped {
                    if !p.ending_fragment {
                        self.selected.store(true, Ordering::SeqCst);
                        self.unordered_is_selected.store(true, Ordering::SeqCst);
                    }
                }
                popped
            } else {
                let popped = {
                    let mut ordered_queue = self.ordered_queue.write();
                    ordered_queue.pop_front()
                };
                if let Some(p) = &popped {
                    if !p.ending_fragment {
                        self.selected.store(true, Ordering::SeqCst);
                        self.unordered_is_selected.store(false, Ordering::SeqCst);
                    }
                }
                popped
            }
        };

        if let Some(p) = &popped {
            let user_data_len = p.user_data.len();
            self.n_bytes.fetch_sub(user_data_len, Ordering::SeqCst);
            self.queue_len.fetch_sub(1, Ordering::SeqCst);
            self.semaphore.release(user_data_len as u64);
        }

        popped
    }

    pub(crate) fn get_num_bytes(&self) -> usize {
        self.n_bytes.load(Ordering::SeqCst)
    }

    pub(crate) fn len(&self) -> usize {
        self.queue_len.load(Ordering::SeqCst)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
