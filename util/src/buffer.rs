use crate::error::Error;

use std::collections::VecDeque;

use tokio::sync::{mpsc, Lock};

#[cfg(test)]
mod buffer_test;

// ErrFull is returned when the buffer has hit the configured limits.
lazy_static! {
    pub static ref ERR_BUFFER_FULL: Error = Error::new("buffer: full".to_owned());
    pub static ref ERR_BUFFER_CLOSED: Error = Error::new("buffer: closed".to_owned());
    pub static ref ERR_BUFFER_UNEXPECTED_EMPTY: Error =
        Error::new("buffer: unexpected empty".to_owned());
    pub static ref ERR_BUFFER_TOO_SHORT: Error = Error::new("buffer: too short".to_owned());
}
// Buffer allows writing packets to an intermediate buffer, which can then be read form.
// This is verify similar to bytes.Buffer but avoids combining multiple writes into a single read.
struct BufferInternal {
    packets: VecDeque<Vec<u8>>,

    notify_tx: mpsc::Sender<()>,
    notify_rx: mpsc::Receiver<()>,

    closed: bool,

    // The number of buffered packets in bytes.
    size: usize,

    // The limit on Write in packet count and total size.
    limit_count: usize,
    limit_size: usize,
}

pub struct Buffer {
    buffer: Lock<BufferInternal>,
}

impl Buffer {
    pub fn new(limit_count: usize, limit_size: usize) -> Result<Self, Error> {
        if limit_count == 0 {
            return Err(Error::new("limit_count must > 0".to_string()));
        }
        let (notify_tx, notify_rx) = mpsc::channel(limit_count);

        Ok(Buffer {
            buffer: Lock::new(BufferInternal {
                packets: VecDeque::new(),

                notify_tx,
                notify_rx,

                closed: false,
                size: 0,

                limit_count,
                limit_size,
            }),
        })
    }

    // Write appends a copy of the packet data to the buffer.
    // If any defined limits are hit, returns ErrFull.
    pub async fn write(&mut self, packet: &[u8]) -> Result<usize, Error> {
        let mut b = self.buffer.lock().await;

        if b.closed {
            return Err(ERR_BUFFER_CLOSED.clone());
        }

        // Check if there is available capacity
        if b.limit_count != 0 && b.packets.len() + 1 > b.limit_count {
            return Err(ERR_BUFFER_FULL.clone());
        }

        // Check if there is available capacity
        if b.limit_size != 0 && b.size + packet.len() > b.limit_size {
            return Err(ERR_BUFFER_FULL.clone());
        }

        b.notify_tx.send(()).await?;

        b.packets.push_back(packet.to_vec());
        b.size += packet.len();

        Ok(packet.len())
    }

    // Read populates the given byte slice, returning the number of bytes read.
    // Blocks until data is available or the buffer is closed.
    // Returns io.ErrShortBuffer is the packet is too small to copy the Write.
    // Returns io.EOF if the buffer is closed.
    pub async fn read(&mut self, packet: &mut [u8]) -> Result<usize, Error> {
        let mut b = self.buffer.lock().await;

        if b.closed {
            return Err(ERR_BUFFER_CLOSED.clone());
        }

        let r = b.notify_rx.recv().await;
        if r.is_none() {
            return Err(ERR_BUFFER_CLOSED.clone());
        }

        let first = b.packets.pop_front();
        if let Some(first) = first {
            if first.len() > packet.len() {
                return Err(ERR_BUFFER_TOO_SHORT.clone());
            }
            packet[0..first.len()].copy_from_slice(&first);
            Ok(first.len())
        } else {
            Err(ERR_BUFFER_UNEXPECTED_EMPTY.clone())
        }
    }

    // Close will unblock any readers and prevent future writes.
    // Data in the buffer can still be read, returning io.EOF when fully depleted.
    pub async fn close(&mut self) {
        // note: We don't use defer so we can close the notify channel after unlocking.
        // This will unblock goroutines that can grab the lock immediately, instead of blocking again.
        let mut b = self.buffer.lock().await;

        if b.closed {
            return;
        }

        b.closed = true;
        b.notify_rx.close();
    }

    // Count returns the number of packets in the buffer.
    pub async fn count(&mut self) -> usize {
        let b = self.buffer.lock().await;

        b.packets.len()
    }

    // Size returns the total byte size of packets in the buffer.
    pub async fn size(&mut self) -> usize {
        let b = self.buffer.lock().await;

        b.size
    }
}
