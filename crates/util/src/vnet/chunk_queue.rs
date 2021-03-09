use super::chunk::*;

use std::collections::VecDeque;
use tokio::sync::RwLock;

pub(crate) struct ChunkQueue {
    chunks: RwLock<VecDeque<Box<dyn Chunk>>>,
    max_size: usize, // 0 or negative value: unlimited
}

impl ChunkQueue {
    pub(crate) fn new(max_size: usize) -> Self {
        ChunkQueue {
            chunks: RwLock::new(VecDeque::new()),
            max_size,
        }
    }

    pub(crate) async fn push(&self, c: Box<dyn Chunk>) -> bool {
        let mut chunks = self.chunks.write().await;

        if self.max_size > 0 && chunks.len() >= self.max_size {
            false // dropped
        } else {
            chunks.push_back(c);
            true
        }
    }

    pub(crate) async fn pop(&self) -> Option<Box<dyn Chunk>> {
        let mut chunks = self.chunks.write().await;
        chunks.pop_front()
    }

    pub(crate) async fn peek(&self) -> Option<Box<dyn Chunk>> {
        let chunks = self.chunks.write().await;
        if let Some(chunk) = chunks.front() {
            Some(chunk.clone_to())
        } else {
            None
        }
    }
}
