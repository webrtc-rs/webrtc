use crate::error::Error;

use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Duration};

#[cfg(test)]
mod buffer_test;

// ErrFull is returned when the buffer has hit the configured limits.
lazy_static! {
    pub static ref ERR_BUFFER_FULL: Error = Error::new("buffer: full".to_owned());
    pub static ref ERR_BUFFER_CLOSED: Error = Error::new("buffer: closed".to_owned());
    pub static ref ERR_BUFFER_SHORT: Error = Error::new("buffer: short".to_owned());
    pub static ref ERR_PACKET_TOO_BIG: Error = Error::new("packet too big".to_owned());
    pub static ref ERR_TIMEOUT: Error = Error::new("i/o timeout".to_owned());
}

const MIN_SIZE: usize = 2048;
const CUTOFF_SIZE: usize = 128 * 1024;
const MAX_SIZE: usize = 4 * 1024 * 1024;

// Buffer allows writing packets to an intermediate buffer, which can then be read form.
// This is verify similar to bytes.Buffer but avoids combining multiple writes into a single read.
#[derive(Debug)]
struct BufferInternal {
    data: Vec<u8>,
    head: usize,
    tail: usize,

    notify_tx: Option<mpsc::Sender<()>>,
    notify_rx: Option<mpsc::Receiver<()>>,
    subs: bool,
    closed: bool,

    count: usize,
    limit_count: usize,
    limit_size: usize,
}

impl BufferInternal {
    // available returns true if the buffer is large enough to fit a packet
    // of the given size, taking overhead into account.
    fn available(&self, size: usize) -> bool {
        let mut available = self.head as isize - self.tail as isize;
        if available <= 0 {
            available += self.data.len() as isize;
        }
        // we interpret head=tail as empty, so always keep a byte free
        size as isize + 2 < available
    }

    // grow increases the size of the buffer.  If it returns nil, then the
    // buffer has been grown.  It returns ErrFull if hits a limit.
    fn grow(&mut self) -> Result<(), Error> {
        let mut newsize = if self.data.len() < CUTOFF_SIZE {
            2 * self.data.len()
        } else {
            5 * self.data.len() / 4
        };

        if newsize < MIN_SIZE {
            newsize = MIN_SIZE
        }
        if (self.limit_size == 0/*|| sizeHardlimit*/) && newsize > MAX_SIZE {
            newsize = MAX_SIZE
        }

        // one byte slack
        if self.limit_size > 0 && newsize > self.limit_size + 1 {
            newsize = self.limit_size + 1
        }

        if newsize <= self.data.len() {
            return Err(ERR_BUFFER_FULL.clone());
        }

        let mut newdata: Vec<u8> = vec![0; newsize];

        let mut n;
        if self.head <= self.tail {
            // data was contiguous
            n = self.tail - self.head;
            newdata[..n].copy_from_slice(&self.data[self.head..self.tail]);
        } else {
            // data was discontiguous
            n = self.data.len() - self.head;
            newdata[..n].copy_from_slice(&self.data[self.head..]);
            newdata[n..n + self.tail].copy_from_slice(&self.data[..self.tail]);
            n += self.tail;
        }
        self.head = 0;
        self.tail = n;
        self.data = newdata;

        Ok(())
    }

    fn size(&self) -> usize {
        let mut size = self.tail as isize - self.head as isize;
        if size < 0 {
            size += self.data.len() as isize;
        }
        size as usize
    }
}

#[derive(Debug, Clone)]
pub struct Buffer {
    buffer: Arc<Mutex<BufferInternal>>,
}

impl Buffer {
    pub fn new(limit_count: usize, limit_size: usize) -> Self {
        let (notify_tx, notify_rx) = mpsc::channel(1);

        Buffer {
            buffer: Arc::new(Mutex::new(BufferInternal {
                data: vec![],
                head: 0,
                tail: 0,

                notify_tx: Some(notify_tx),
                notify_rx: Some(notify_rx),

                subs: false,
                closed: false,

                count: 0,
                limit_count,
                limit_size,
            })),
        }
    }

    // Write appends a copy of the packet data to the buffer.
    // Returns ErrFull if the packet doesn't fit.
    // Note that the packet size is limited to 65536 bytes since v0.11.0
    // due to the internal data structure.
    pub async fn write(&self, packet: &[u8]) -> Result<usize, Error> {
        if packet.len() >= 0x10000 {
            return Err(ERR_PACKET_TOO_BIG.clone());
        }

        let mut b = self.buffer.lock().await;

        if b.closed {
            return Err(ERR_BUFFER_CLOSED.clone());
        }

        if (b.limit_count > 0 && b.count >= b.limit_count)
            || (b.limit_size > 0 && b.size() + 2 + packet.len() > b.limit_size)
        {
            return Err(ERR_BUFFER_FULL.clone());
        }

        // grow the buffer until the packet fits
        while !b.available(packet.len()) {
            b.grow()?;
        }

        let mut notify = if b.subs {
            // readers are waiting.  Prepare to notify, but only
            // actually do it after we release the lock.
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

        // store the length of the packet
        let tail = b.tail;
        b.data[tail] = (packet.len() >> 8) as u8;
        b.tail += 1;
        if b.tail >= b.data.len() {
            b.tail = 0;
        }

        let tail = b.tail;
        b.data[tail] = packet.len() as u8;
        b.tail += 1;
        if b.tail >= b.data.len() {
            b.tail = 0;
        }

        // store the packet
        let end = std::cmp::min(b.data.len(), b.tail + packet.len());
        let n = end - b.tail;
        let tail = b.tail;
        b.data[tail..end].copy_from_slice(&packet[..n]);
        b.tail += n;
        if b.tail >= b.data.len() {
            // we reached the end, wrap around
            let m = packet.len() - n;
            b.data[..m].copy_from_slice(&packet[n..]);
            b.tail = m;
        }
        b.count += 1;

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
    pub async fn read(
        &self,
        packet: &mut [u8],
        duration: Option<Duration>,
    ) -> Result<usize, Error> {
        loop {
            let notify;
            {
                // use {} to let LockGuard RAII
                let mut b = self.buffer.lock().await;

                if b.head != b.tail {
                    // decode the packet size
                    let n1 = b.data[b.head];
                    b.head += 1;
                    if b.head >= b.data.len() {
                        b.head = 0;
                    }
                    let n2 = b.data[b.head];
                    b.head += 1;
                    if b.head >= b.data.len() {
                        b.head = 0;
                    }
                    let count = ((n1 as usize) << 8) | n2 as usize;

                    // determine the number of bytes we'll actually copy
                    let mut copied = count;
                    if copied > packet.len() {
                        copied = packet.len();
                    }

                    // copy the data
                    if b.head + copied < b.data.len() {
                        packet[..copied].copy_from_slice(&b.data[b.head..b.head + copied]);
                    } else {
                        let k = b.data.len() - b.head;
                        packet[..k].copy_from_slice(&b.data[b.head..]);
                        packet[k..copied].copy_from_slice(&b.data[..copied - k]);
                    }

                    // advance head, discarding any data that wasn't copied
                    b.head += count;
                    if b.head >= b.data.len() {
                        b.head -= b.data.len();
                    }

                    if b.head == b.tail {
                        // the buffer is empty, reset to beginning
                        // in order to improve cache locality.
                        b.head = 0;
                        b.tail = 0;
                    }

                    b.count -= 1;

                    if copied < count {
                        return Err(ERR_BUFFER_SHORT.clone());
                    }
                    return Ok(copied);
                }

                if b.closed {
                    return Err(ERR_BUFFER_CLOSED.clone());
                }

                notify = b.notify_rx.take();

                // Set the subs marker, telling the writer we're waiting.
                b.subs = true
            }

            // Wake for the broadcast.
            if let Some(mut notify) = notify {
                if let Some(d) = duration {
                    if timeout(d, notify.recv()).await.is_err() {
                        return Err(ERR_TIMEOUT.clone());
                    }
                } else {
                    notify.recv().await;
                }
            }
        }
    }

    // Close will unblock any readers and prevent future writes.
    // Data in the buffer can still be read, returning io.EOF when fully depleted.
    pub async fn close(&self) {
        // note: We don't use defer so we can close the notify channel after unlocking.
        // This will unblock goroutines that can grab the lock immediately, instead of blocking again.
        let mut b = self.buffer.lock().await;

        if b.closed {
            return;
        }

        b.notify_tx.take();
        b.closed = true;
    }

    pub async fn is_closed(&self) -> bool {
        let b = self.buffer.lock().await;

        b.closed
    }

    // Count returns the number of packets in the buffer.
    pub async fn count(&self) -> usize {
        let b = self.buffer.lock().await;

        b.count
    }

    // set_limit_count controls the maximum number of packets that can be buffered.
    // Causes Write to return ErrFull when this limit is reached.
    // A zero value will disable this limit.
    pub async fn set_limit_count(&self, limit: usize) {
        let mut b = self.buffer.lock().await;

        b.limit_count = limit
    }

    // Size returns the total byte size of packets in the buffer.
    pub async fn size(&self) -> usize {
        let b = self.buffer.lock().await;

        b.size()
    }

    // set_limit_size controls the maximum number of bytes that can be buffered.
    // Causes Write to return ErrFull when this limit is reached.
    // A zero value means 4MB since v0.11.0.
    //
    // User can set packetioSizeHardlimit build tag to enable 4MB hardlimit.
    // When packetioSizeHardlimit build tag is set, set_limit_size exceeding
    // the hardlimit will be silently discarded.
    pub async fn set_limit_size(&self, limit: usize) {
        let mut b = self.buffer.lock().await;

        b.limit_size = limit
    }
}
