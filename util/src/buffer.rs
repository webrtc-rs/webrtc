use crate::error::Error;

use std::collections::VecDeque;

use tokio::sync::{mpsc, Lock};

#[cfg(test)]
mod buffer_test;

// ErrFull is returned when the buffer has hit the configured limits.
lazy_static! {
    pub static ref ERR_BUFFER_FULL: Error = Error::new("buffer: full".to_owned());
    pub static ref ERR_BUFFER_CLOSED: Error = Error::new("buffer: closed".to_owned());
    pub static ref ERR_BUFFER_SHORT: Error = Error::new("buffer: short".to_owned());
}
// Buffer allows writing packets to an intermediate buffer, which can then be read form.
// This is verify similar to bytes.Buffer but avoids combining multiple writes into a single read.
struct BufferInternal {
    packets: VecDeque<Vec<u8>>,

    notify_tx: Option<mpsc::Sender<()>>,
    notify_rx: Option<mpsc::Receiver<()>>,

    subs: bool,
    closed: bool,

    // The number of buffered packets in bytes.
    size: usize,

    // The limit on Write in packet count and total size.
    limit_count: usize,
    limit_size: usize,
}

#[derive(Clone)]
pub struct Buffer {
    buffer: Lock<BufferInternal>,
}

impl Buffer {
    pub fn new(limit_count: usize, limit_size: usize) -> Self {
        let (notify_tx, notify_rx) = mpsc::channel(1);

        Buffer {
            buffer: Lock::new(BufferInternal {
                packets: VecDeque::new(),

                notify_tx: Some(notify_tx),
                notify_rx: Some(notify_rx),

                subs: false,
                closed: false,
                size: 0,

                limit_count,
                limit_size,
            }),
        }
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

        // Decide if we need to wake up any readers.
        let mut notify = if b.subs {
            // If so, close the notify channel and make a new one.
            // This effectively behaves like a broadcast, waking up any blocked goroutines.
            // We close after we release the lock to reduce contention.
            let notify = b.notify_tx.take();

            let (notify_tx, notify_rx) = mpsc::channel(1);

            b.notify_tx = Some(notify_tx);
            b.notify_rx = Some(notify_rx);

            // Reset the subs marker.
            b.subs = false;

            notify
        } else {
            None
        };

        b.packets.push_back(packet.to_vec());
        b.size += packet.len();

        // Actually close the notify channel down here.
        if notify.is_some() {
            notify.take(); //drop notify
        }

        Ok(packet.len())
    }

    // Read populates the given byte slice, returning the number of bytes read.
    // Blocks until data is available or the buffer is closed.
    // Returns io.ErrShortBuffer is the packet is too small to copy the Write.
    // Returns io.EOF if the buffer is closed.
    pub async fn read(&mut self, packet: &mut [u8]) -> Result<usize, Error> {
        loop {
            let notify;
            {
                // use {} to let LockGuard RAII
                let mut b = self.buffer.lock().await;

                // See if there are any packets in the queue.
                if !b.packets.is_empty() {
                    if let Some(first) = b.packets.front() {
                        // This is a packet-based reader/writer so we can't truncate.
                        if first.len() > packet.len() {
                            return Err(ERR_BUFFER_SHORT.clone());
                        }
                    }

                    if let Some(first) = b.packets.pop_front() {
                        b.size -= first.len();

                        packet[0..first.len()].copy_from_slice(&first);
                        return Ok(first.len());
                    }
                }

                // Make sure the reader isn't actually closed.
                // This is done after checking packets to fully read the buffer.
                if b.closed {
                    return Err(ERR_BUFFER_CLOSED.clone());
                }

                notify = b.notify_rx.take();

                // Set the subs marker, telling the writer we're waiting.
                b.subs = true
            }

            // Wake for the broadcast.
            if let Some(mut notify) = notify {
                notify.recv().await;
            }
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

        b.notify_tx.take();
        b.closed = true;
    }

    pub async fn is_closed(&mut self) -> bool {
        let b = self.buffer.lock().await;

        b.closed
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
