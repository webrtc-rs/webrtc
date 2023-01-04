use std::{
    collections::VecDeque,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use tokio::sync::{Mutex, Semaphore};
use util::sync::RwLock;

use crate::chunk::chunk_payload_data::ChunkPayloadData;

/// Basic queue for either ordered or unordered chunks.
pub(crate) type PendingBaseQueue = VecDeque<ChunkPayloadData>;

// TODO: benchmark performance between multiple Atomic+Mutex vs one Mutex<PendingQueueInternal>

/// A queue for both ordered and unordered chunks.
#[derive(Debug)]
pub(crate) struct PendingQueue {
    // These two fields limit appending bytes to the queue
    // This two step process is necessary because
    // A) We need backpressure which the semaphore applies by limiting the total amount of bytes via the permits
    // B) The chunks of one fragmented message need to be put in direct sequence into the queue which the lock guarantees
    //
    // The semaphore is not inside the lock because the permits need to be returned without needing a lock on the semaphore
    semaphore_lock: Mutex<()>,
    semaphore: Semaphore,

    unordered_queue: RwLock<PendingBaseQueue>,
    ordered_queue: RwLock<PendingBaseQueue>,
    queue_len: AtomicUsize,
    n_bytes: AtomicUsize,
    selected: AtomicBool,
    unordered_is_selected: AtomicBool,
}

impl Default for PendingQueue {
    fn default() -> Self {
        PendingQueue::new()
    }
}

// Some tests push a lot of data before starting to process any data...
#[cfg(test)]
const QUEUE_BYTES_LIMIT: usize = 128 * 1024 * 1024;
#[cfg(not(test))]
const QUEUE_BYTES_LIMIT: usize = 128 * 1024;

const QUEUE_APPEND_LARGE: usize = (QUEUE_BYTES_LIMIT * 2) / 3;

impl PendingQueue {
    pub(crate) fn new() -> Self {
        Self {
            semaphore_lock: Mutex::default(),
            semaphore: Semaphore::new(QUEUE_BYTES_LIMIT),
            unordered_queue: Default::default(),
            ordered_queue: Default::default(),
            queue_len: Default::default(),
            n_bytes: Default::default(),
            selected: Default::default(),
            unordered_is_selected: Default::default(),
        }
    }

    /// Appends a chunk to the back of the pending queue.
    pub(crate) async fn push(&self, c: ChunkPayloadData) {
        let user_data_len = c.user_data.len();

        {
            let sem_lock = self.semaphore_lock.lock().await;
            let permits = self.semaphore.acquire_many(user_data_len as u32).await;
            // unwrap ok because we never close the semaphore unless we have dropped self
            permits.unwrap().forget();

            if c.unordered {
                let mut unordered_queue = self.unordered_queue.write();
                unordered_queue.push_back(c);
            } else {
                let mut ordered_queue = self.ordered_queue.write();
                ordered_queue.push_back(c);
            }
            drop(sem_lock);
        }

        self.n_bytes.fetch_add(user_data_len, Ordering::SeqCst);
        self.queue_len.fetch_add(1, Ordering::SeqCst);
    }

    /// Appends chunks to the back of the pending queue.
    ///
    /// # Panics
    ///
    /// If it's a mix of unordered and ordered chunks.
    pub(crate) async fn append(&self, chunks: Vec<ChunkPayloadData>) {
        if chunks.is_empty() {
            return;
        }

        let total_user_data_len = chunks.iter().fold(0, |acc, c| acc + c.user_data.len());

        if total_user_data_len >= QUEUE_APPEND_LARGE {
            self.append_large(chunks).await
        } else {
            let sem_lock = self.semaphore_lock.lock().await;
            let permits = self
                .semaphore
                .acquire_many(total_user_data_len as u32)
                .await;
            // unwrap ok because we never close the semaphore unless we have dropped self
            permits.unwrap().forget();
            self.append_unlimited(chunks, total_user_data_len);
            drop(sem_lock);
        }
    }

    // If this is a very large message we append chunks one by one to allow progress while we are appending
    async fn append_large(&self, chunks: Vec<ChunkPayloadData>) {
        // lock this for the whole duration
        let sem_lock = self.semaphore_lock.lock().await;

        for chunk in chunks.into_iter() {
            let user_data_len = chunk.user_data.len();
            let permits = self.semaphore.acquire_many(user_data_len as u32).await;
            // unwrap ok because we never close the semaphore unless we have dropped self
            permits.unwrap().forget();

            if chunk.unordered {
                let mut unordered_queue = self.unordered_queue.write();
                unordered_queue.push_back(chunk);
            } else {
                let mut ordered_queue = self.ordered_queue.write();
                ordered_queue.push_back(chunk);
            }
            self.n_bytes.fetch_add(user_data_len, Ordering::SeqCst);
            self.queue_len.fetch_add(1, Ordering::SeqCst);
        }

        drop(sem_lock);
    }

    /// Assumes that A) enough permits have been acquired and forget from the semaphore and that the semaphore_lock is held
    fn append_unlimited(&self, chunks: Vec<ChunkPayloadData>, total_user_data_len: usize) {
        let chunks_len = chunks.len();
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
            self.semaphore.add_permits(user_data_len);
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
