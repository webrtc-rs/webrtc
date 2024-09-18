#[cfg(test)]
mod stream_test;

use std::future::Future;
use std::net::Shutdown;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::{fmt, io};

use arc_swap::ArcSwapOption;
use bytes::Bytes;
use portable_atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU8, AtomicUsize};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, Mutex, Notify};

use crate::association::AssociationState;
use crate::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::error::{Error, Result};
use crate::queue::pending_queue::PendingQueue;
use crate::queue::reassembly_queue::ReassemblyQueue;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum ReliabilityType {
    /// ReliabilityTypeReliable is used for reliable transmission
    #[default]
    Reliable = 0,
    /// ReliabilityTypeRexmit is used for partial reliability by retransmission count
    Rexmit = 1,
    /// ReliabilityTypeTimed is used for partial reliability by retransmission duration
    Timed = 2,
}

impl fmt::Display for ReliabilityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ReliabilityType::Reliable => "Reliable",
            ReliabilityType::Rexmit => "Rexmit",
            ReliabilityType::Timed => "Timed",
        };
        write!(f, "{s}")
    }
}

impl From<u8> for ReliabilityType {
    fn from(v: u8) -> ReliabilityType {
        match v {
            1 => ReliabilityType::Rexmit,
            2 => ReliabilityType::Timed,
            _ => ReliabilityType::Reliable,
        }
    }
}

pub type OnBufferedAmountLowFn =
    Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

// TODO: benchmark performance between multiple Atomic+Mutex vs one Mutex<StreamInternal>

/// Stream represents an SCTP stream
pub struct Stream {
    pub(crate) max_payload_size: u32,
    pub(crate) max_message_size: Arc<AtomicU32>, // clone from association
    pub(crate) state: Arc<AtomicU8>,             // clone from association
    pub(crate) awake_write_loop_ch: Arc<mpsc::Sender<()>>,
    pub(crate) pending_queue: Arc<PendingQueue>,

    pub(crate) stream_identifier: u16,
    pub(crate) default_payload_type: AtomicU32, //PayloadProtocolIdentifier,
    pub(crate) reassembly_queue: Mutex<ReassemblyQueue>,
    pub(crate) sequence_number: AtomicU16,
    pub(crate) read_notifier: Notify,
    pub(crate) read_shutdown: AtomicBool,
    pub(crate) write_shutdown: AtomicBool,
    pub(crate) unordered: AtomicBool,
    pub(crate) reliability_type: AtomicU8, //ReliabilityType,
    pub(crate) reliability_value: AtomicU32,
    pub(crate) buffered_amount: AtomicUsize,
    pub(crate) buffered_amount_low: AtomicUsize,
    pub(crate) on_buffered_amount_low: ArcSwapOption<Mutex<OnBufferedAmountLowFn>>,
    pub(crate) name: String,
}

impl fmt::Debug for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stream")
            .field("max_payload_size", &self.max_payload_size)
            .field("max_message_size", &self.max_message_size)
            .field("state", &self.state)
            .field("awake_write_loop_ch", &self.awake_write_loop_ch)
            .field("stream_identifier", &self.stream_identifier)
            .field("default_payload_type", &self.default_payload_type)
            .field("reassembly_queue", &self.reassembly_queue)
            .field("sequence_number", &self.sequence_number)
            .field("read_shutdown", &self.read_shutdown)
            .field("write_shutdown", &self.write_shutdown)
            .field("unordered", &self.unordered)
            .field("reliability_type", &self.reliability_type)
            .field("reliability_value", &self.reliability_value)
            .field("buffered_amount", &self.buffered_amount)
            .field("buffered_amount_low", &self.buffered_amount_low)
            .field("name", &self.name)
            .finish()
    }
}

impl Stream {
    pub(crate) fn new(
        name: String,
        stream_identifier: u16,
        max_payload_size: u32,
        max_message_size: Arc<AtomicU32>,
        state: Arc<AtomicU8>,
        awake_write_loop_ch: Arc<mpsc::Sender<()>>,
        pending_queue: Arc<PendingQueue>,
    ) -> Self {
        Self {
            max_payload_size,
            max_message_size,
            state,
            awake_write_loop_ch,
            pending_queue,

            stream_identifier,
            default_payload_type: AtomicU32::new(0), //PayloadProtocolIdentifier::Unknown,
            reassembly_queue: Mutex::new(ReassemblyQueue::new(stream_identifier)),
            sequence_number: AtomicU16::new(0),
            read_notifier: Notify::new(),
            read_shutdown: AtomicBool::new(false),
            write_shutdown: AtomicBool::new(false),
            unordered: AtomicBool::new(false),
            reliability_type: AtomicU8::new(0), //ReliabilityType::Reliable,
            reliability_value: AtomicU32::new(0),
            buffered_amount: AtomicUsize::new(0),
            buffered_amount_low: AtomicUsize::new(0),
            on_buffered_amount_low: ArcSwapOption::empty(),
            name,
        }
    }

    /// stream_identifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> u16 {
        self.stream_identifier
    }

    /// set_default_payload_type sets the default payload type used by write.
    pub fn set_default_payload_type(&self, default_payload_type: PayloadProtocolIdentifier) {
        self.default_payload_type
            .store(default_payload_type as u32, Ordering::SeqCst);
    }

    /// set_reliability_params sets reliability parameters for this stream.
    pub fn set_reliability_params(&self, unordered: bool, rel_type: ReliabilityType, rel_val: u32) {
        log::debug!(
            "[{}] reliability params: ordered={} type={} value={}",
            self.name,
            !unordered,
            rel_type,
            rel_val
        );
        self.unordered.store(unordered, Ordering::SeqCst);
        self.reliability_type
            .store(rel_type as u8, Ordering::SeqCst);
        self.reliability_value.store(rel_val, Ordering::SeqCst);
    }

    /// Reads a packet of len(p) bytes, dropping the Payload Protocol Identifier.
    ///
    /// Returns `Error::ErrShortBuffer` if `p` is too short.
    /// Returns `0` if the reading half of this stream is shutdown or it (the stream) was reset.
    pub async fn read(&self, p: &mut [u8]) -> Result<usize> {
        let (n, _) = self.read_sctp(p).await?;
        Ok(n)
    }

    /// Reads a packet of len(p) bytes and returns the associated Payload Protocol Identifier.
    ///
    /// Returns `Error::ErrShortBuffer` if `p` is too short.
    /// Returns `(0, PayloadProtocolIdentifier::Unknown)` if the reading half of this stream is shutdown or it (the stream) was reset.
    pub async fn read_sctp(&self, p: &mut [u8]) -> Result<(usize, PayloadProtocolIdentifier)> {
        loop {
            if self.read_shutdown.load(Ordering::SeqCst) {
                return Ok((0, PayloadProtocolIdentifier::Unknown));
            }

            let result = {
                let mut reassembly_queue = self.reassembly_queue.lock().await;
                reassembly_queue.read(p)
            };

            match result {
                Ok(_) | Err(Error::ErrShortBuffer { .. }) => return result,
                Err(_) => {
                    // wait for the next chunk to become available
                    self.read_notifier.notified().await;
                }
            }
        }
    }

    pub(crate) async fn handle_data(&self, pd: ChunkPayloadData) {
        let readable = {
            let mut reassembly_queue = self.reassembly_queue.lock().await;
            if reassembly_queue.push(pd) {
                let readable = reassembly_queue.is_readable();
                log::debug!("[{}] reassemblyQueue readable={}", self.name, readable);
                readable
            } else {
                false
            }
        };

        if readable {
            log::debug!("[{}] readNotifier.signal()", self.name);
            self.read_notifier.notify_one();
            log::debug!("[{}] readNotifier.signal() done", self.name);
        }
    }

    pub(crate) async fn handle_forward_tsn_for_ordered(&self, ssn: u16) {
        if self.unordered.load(Ordering::SeqCst) {
            return; // unordered chunks are handled by handleForwardUnordered method
        }

        // Remove all chunks older than or equal to the new TSN from
        // the reassembly_queue.
        let readable = {
            let mut reassembly_queue = self.reassembly_queue.lock().await;
            reassembly_queue.forward_tsn_for_ordered(ssn);
            reassembly_queue.is_readable()
        };

        // Notify the reader asynchronously if there's a data chunk to read.
        if readable {
            self.read_notifier.notify_one();
        }
    }

    pub(crate) async fn handle_forward_tsn_for_unordered(&self, new_cumulative_tsn: u32) {
        if !self.unordered.load(Ordering::SeqCst) {
            return; // ordered chunks are handled by handleForwardTSNOrdered method
        }

        // Remove all chunks older than or equal to the new TSN from
        // the reassembly_queue.
        let readable = {
            let mut reassembly_queue = self.reassembly_queue.lock().await;
            reassembly_queue.forward_tsn_for_unordered(new_cumulative_tsn);
            reassembly_queue.is_readable()
        };

        // Notify the reader asynchronously if there's a data chunk to read.
        if readable {
            self.read_notifier.notify_one();
        }
    }

    /// Writes `p` to the DTLS connection with the default Payload Protocol Identifier.
    ///
    /// Returns an error if the write half of this stream is shutdown or `p` is too large.
    pub async fn write(&self, p: &Bytes) -> Result<usize> {
        self.write_sctp(p, self.default_payload_type.load(Ordering::SeqCst).into())
            .await
    }

    /// Writes `p` to the DTLS connection with the given Payload Protocol Identifier.
    ///
    /// Returns an error if the write half of this stream is shutdown or `p` is too large.
    pub async fn write_sctp(&self, p: &Bytes, ppi: PayloadProtocolIdentifier) -> Result<usize> {
        let chunks = self.prepare_write(p, ppi)?;
        self.send_payload_data(chunks).await?;

        Ok(p.len())
    }

    /// common stuff for write and try_write
    fn prepare_write(
        &self,
        p: &Bytes,
        ppi: PayloadProtocolIdentifier,
    ) -> Result<Vec<ChunkPayloadData>> {
        if self.write_shutdown.load(Ordering::SeqCst) {
            return Err(Error::ErrStreamClosed);
        }

        if p.len() > self.max_message_size.load(Ordering::SeqCst) as usize {
            return Err(Error::ErrOutboundPacketTooLarge);
        }

        let state: AssociationState = self.state.load(Ordering::SeqCst).into();
        match state {
            AssociationState::ShutdownSent
            | AssociationState::ShutdownAckSent
            | AssociationState::ShutdownPending
            | AssociationState::ShutdownReceived => return Err(Error::ErrStreamClosed),
            _ => {}
        };

        Ok(self.packetize(p, ppi))
    }

    fn packetize(&self, raw: &Bytes, ppi: PayloadProtocolIdentifier) -> Vec<ChunkPayloadData> {
        let mut i = 0;
        let mut remaining = raw.len();

        // From draft-ietf-rtcweb-data-protocol-09, section 6:
        //   All Data Channel Establishment Protocol messages MUST be sent using
        //   ordered delivery and reliable transmission.
        let unordered =
            ppi != PayloadProtocolIdentifier::Dcep && self.unordered.load(Ordering::SeqCst);

        let mut chunks = vec![];

        let head_abandoned = Arc::new(AtomicBool::new(false));
        let head_all_inflight = Arc::new(AtomicBool::new(false));
        while remaining != 0 {
            let fragment_size = std::cmp::min(self.max_payload_size as usize, remaining); //self.association.max_payload_size

            // Copy the userdata since we'll have to store it until acked
            // and the caller may re-use the buffer in the mean time
            let user_data = raw.slice(i..i + fragment_size);

            let chunk = ChunkPayloadData {
                stream_identifier: self.stream_identifier,
                user_data,
                unordered,
                beginning_fragment: i == 0,
                ending_fragment: remaining - fragment_size == 0,
                immediate_sack: false,
                payload_type: ppi,
                stream_sequence_number: self.sequence_number.load(Ordering::SeqCst),
                abandoned: head_abandoned.clone(), // all fragmented chunks use the same abandoned
                all_inflight: head_all_inflight.clone(), // all fragmented chunks use the same all_inflight
                ..Default::default()
            };

            chunks.push(chunk);

            remaining -= fragment_size;
            i += fragment_size;
        }

        // RFC 4960 Sec 6.6
        // Note: When transmitting ordered and unordered data, an endpoint does
        // not increment its Stream Sequence Number when transmitting a DATA
        // chunk with U flag set to 1.
        if !unordered {
            self.sequence_number.fetch_add(1, Ordering::SeqCst);
        }

        let old_value = self.buffered_amount.fetch_add(raw.len(), Ordering::SeqCst);
        log::trace!("[{}] bufferedAmount = {}", self.name, old_value + raw.len());

        chunks
    }

    /// Closes both read and write halves of this stream.
    ///
    /// Use [`Stream::shutdown`] instead.
    #[deprecated]
    pub async fn close(&self) -> Result<()> {
        self.shutdown(Shutdown::Both).await
    }

    /// Shuts down the read, write, or both halves of this stream.
    ///
    /// This function will cause all pending and future I/O on the specified portions to return
    /// immediately with an appropriate value (see the documentation of [`Shutdown`]).
    ///
    /// Resets the stream when both halves of this stream are shutdown.
    pub async fn shutdown(&self, how: Shutdown) -> Result<()> {
        if self.read_shutdown.load(Ordering::SeqCst) && self.write_shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }

        if how == Shutdown::Write || how == Shutdown::Both {
            self.write_shutdown.store(true, Ordering::SeqCst);
        }

        if (how == Shutdown::Read || how == Shutdown::Both)
            && !self.read_shutdown.swap(true, Ordering::SeqCst)
        {
            self.read_notifier.notify_waiters();
        }

        if how == Shutdown::Both
            || (self.read_shutdown.load(Ordering::SeqCst)
                && self.write_shutdown.load(Ordering::SeqCst))
        {
            // Reset the stream
            // https://tools.ietf.org/html/rfc6525
            self.send_reset_request(self.stream_identifier).await?;
        }

        Ok(())
    }

    /// buffered_amount returns the number of bytes of data currently queued to be sent over this stream.
    pub fn buffered_amount(&self) -> usize {
        self.buffered_amount.load(Ordering::SeqCst)
    }

    /// buffered_amount_low_threshold returns the number of bytes of buffered outgoing data that is
    /// considered "low." Defaults to 0.
    pub fn buffered_amount_low_threshold(&self) -> usize {
        self.buffered_amount_low.load(Ordering::SeqCst)
    }

    /// set_buffered_amount_low_threshold is used to update the threshold.
    /// See buffered_amount_low_threshold().
    pub fn set_buffered_amount_low_threshold(&self, th: usize) {
        self.buffered_amount_low.store(th, Ordering::SeqCst);
    }

    /// on_buffered_amount_low sets the callback handler which would be called when the number of
    /// bytes of outgoing data buffered is lower than the threshold.
    pub fn on_buffered_amount_low(&self, f: OnBufferedAmountLowFn) {
        self.on_buffered_amount_low
            .store(Some(Arc::new(Mutex::new(f))));
    }

    /// This method is called by association's read_loop (go-)routine to notify this stream
    /// of the specified amount of outgoing data has been delivered to the peer.
    pub(crate) async fn on_buffer_released(&self, n_bytes_released: i64) {
        if n_bytes_released <= 0 {
            return;
        }

        let from_amount = self.buffered_amount.load(Ordering::SeqCst);
        let new_amount = if from_amount < n_bytes_released as usize {
            self.buffered_amount.store(0, Ordering::SeqCst);
            log::error!(
                "[{}] released buffer size {} should be <= {}",
                self.name,
                n_bytes_released,
                0,
            );
            0
        } else {
            self.buffered_amount
                .fetch_sub(n_bytes_released as usize, Ordering::SeqCst);

            from_amount - n_bytes_released as usize
        };

        let buffered_amount_low = self.buffered_amount_low.load(Ordering::SeqCst);

        log::trace!(
            "[{}] bufferedAmount = {}, from_amount = {}, buffered_amount_low = {}",
            self.name,
            new_amount,
            from_amount,
            buffered_amount_low,
        );

        if from_amount > buffered_amount_low && new_amount <= buffered_amount_low {
            if let Some(handler) = &*self.on_buffered_amount_low.load() {
                let mut f = handler.lock().await;
                f().await;
            }
        }
    }

    /// get_num_bytes_in_reassembly_queue returns the number of bytes of data currently queued to
    /// be read (once chunk is complete).
    pub(crate) async fn get_num_bytes_in_reassembly_queue(&self) -> usize {
        // No lock is required as it reads the size with atomic load function.
        let reassembly_queue = self.reassembly_queue.lock().await;
        reassembly_queue.get_num_bytes()
    }

    /// get_state atomically returns the state of the Association.
    fn get_state(&self) -> AssociationState {
        self.state.load(Ordering::SeqCst).into()
    }

    fn awake_write_loop(&self) {
        //log::debug!("[{}] awake_write_loop_ch.notify_one", self.name);
        let _ = self.awake_write_loop_ch.try_send(());
    }

    async fn send_payload_data(&self, chunks: Vec<ChunkPayloadData>) -> Result<()> {
        let state = self.get_state();
        if state != AssociationState::Established {
            return Err(Error::ErrPayloadDataStateNotExist);
        }

        // NOTE: append is used here instead of push in order to prevent chunks interlacing.
        self.pending_queue.append(chunks).await;

        self.awake_write_loop();
        Ok(())
    }

    async fn send_reset_request(&self, stream_identifier: u16) -> Result<()> {
        let state = self.get_state();
        if state != AssociationState::Established {
            return Err(Error::ErrResetPacketInStateNotExist);
        }

        // Create DATA chunk which only contains valid stream identifier with
        // nil userData and use it as a EOS from the stream.
        let c = ChunkPayloadData {
            stream_identifier,
            beginning_fragment: true,
            ending_fragment: true,
            user_data: Bytes::new(),
            ..Default::default()
        };

        self.pending_queue.push(c).await;

        self.awake_write_loop();
        Ok(())
    }
}

/// Default capacity of the temporary read buffer used by [`PollStream`].
const DEFAULT_READ_BUF_SIZE: usize = 8192;

/// State of the read `Future` in [`PollStream`].
enum ReadFut {
    /// Nothing in progress.
    Idle,
    /// Reading data from the underlying stream.
    Reading(Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send>>),
    /// Finished reading, but there's unread data in the temporary buffer.
    RemainingData(Vec<u8>),
}

enum ShutdownFut {
    /// Nothing in progress.
    Idle,
    /// Reading data from the underlying stream.
    ShuttingDown(Pin<Box<dyn Future<Output = std::result::Result<(), crate::error::Error>>>>),
    /// Shutdown future has run
    Done,
    Errored(crate::error::Error),
}

impl ReadFut {
    /// Gets a mutable reference to the future stored inside `Reading(future)`.
    ///
    /// # Panics
    ///
    /// Panics if `ReadFut` variant is not `Reading`.
    fn get_reading_mut(&mut self) -> &mut Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send>> {
        match self {
            ReadFut::Reading(ref mut fut) => fut,
            _ => panic!("expected ReadFut to be Reading"),
        }
    }
}

impl ShutdownFut {
    /// Gets a mutable reference to the future stored inside `ShuttingDown(future)`.
    ///
    /// # Panics
    ///
    /// Panics if `ShutdownFut` variant is not `ShuttingDown`.
    fn get_shutting_down_mut(
        &mut self,
    ) -> &mut Pin<Box<dyn Future<Output = std::result::Result<(), crate::error::Error>>>> {
        match self {
            ShutdownFut::ShuttingDown(ref mut fut) => fut,
            _ => panic!("expected ShutdownFut to be ShuttingDown"),
        }
    }
}

/// A wrapper around around [`Stream`], which implements [`AsyncRead`] and
/// [`AsyncWrite`].
///
/// Both `poll_read` and `poll_write` calls allocate temporary buffers, which results in an
/// additional overhead.
pub struct PollStream {
    stream: Arc<Stream>,

    read_fut: ReadFut,
    write_fut: Option<Pin<Box<dyn Future<Output = Result<usize>>>>>,
    shutdown_fut: ShutdownFut,

    read_buf_cap: usize,
}

impl PollStream {
    /// Constructs a new `PollStream`.
    pub fn new(stream: Arc<Stream>) -> Self {
        Self {
            stream,
            read_fut: ReadFut::Idle,
            write_fut: None,
            shutdown_fut: ShutdownFut::Idle,
            read_buf_cap: DEFAULT_READ_BUF_SIZE,
        }
    }

    /// Get back the inner stream.
    #[must_use]
    pub fn into_inner(self) -> Arc<Stream> {
        self.stream
    }

    /// Obtain a clone of the inner stream.
    #[must_use]
    pub fn clone_inner(&self) -> Arc<Stream> {
        self.stream.clone()
    }

    /// stream_identifier returns the Stream identifier associated to the stream.
    pub fn stream_identifier(&self) -> u16 {
        self.stream.stream_identifier
    }

    /// buffered_amount returns the number of bytes of data currently queued to be sent over this stream.
    pub fn buffered_amount(&self) -> usize {
        self.stream.buffered_amount.load(Ordering::SeqCst)
    }

    /// buffered_amount_low_threshold returns the number of bytes of buffered outgoing data that is
    /// considered "low." Defaults to 0.
    pub fn buffered_amount_low_threshold(&self) -> usize {
        self.stream.buffered_amount_low.load(Ordering::SeqCst)
    }

    /// get_num_bytes_in_reassembly_queue returns the number of bytes of data currently queued to
    /// be read (once chunk is complete).
    pub(crate) async fn get_num_bytes_in_reassembly_queue(&self) -> usize {
        // No lock is required as it reads the size with atomic load function.
        let reassembly_queue = self.stream.reassembly_queue.lock().await;
        reassembly_queue.get_num_bytes()
    }

    /// Set the capacity of the temporary read buffer (default: 8192).
    pub fn set_read_buf_capacity(&mut self, capacity: usize) {
        self.read_buf_cap = capacity
    }
}

impl AsyncRead for PollStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        let fut = match self.read_fut {
            ReadFut::Idle => {
                // read into a temporary buffer because `buf` has an unonymous lifetime, which can
                // be shorter than the lifetime of `read_fut`.
                let stream = self.stream.clone();
                let mut temp_buf = vec![0; self.read_buf_cap];
                self.read_fut = ReadFut::Reading(Box::pin(async move {
                    stream.read(temp_buf.as_mut_slice()).await.map(|n| {
                        temp_buf.truncate(n);
                        temp_buf
                    })
                }));
                self.read_fut.get_reading_mut()
            }
            ReadFut::Reading(ref mut fut) => fut,
            ReadFut::RemainingData(ref mut data) => {
                let remaining = buf.remaining();
                let len = std::cmp::min(data.len(), remaining);
                buf.put_slice(&data[..len]);
                if data.len() > remaining {
                    // ReadFut remains to be RemainingData
                    data.drain(0..len);
                } else {
                    self.read_fut = ReadFut::Idle;
                }
                return Poll::Ready(Ok(()));
            }
        };

        loop {
            match fut.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                // retry immediately upon empty data or incomplete chunks
                // since there's no way to setup a waker.
                Poll::Ready(Err(Error::ErrTryAgain)) => {}
                // EOF has been reached => don't touch buf and just return Ok
                Poll::Ready(Err(Error::ErrEof)) => {
                    self.read_fut = ReadFut::Idle;
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Err(e)) => {
                    self.read_fut = ReadFut::Idle;
                    return Poll::Ready(Err(e.into()));
                }
                Poll::Ready(Ok(mut temp_buf)) => {
                    let remaining = buf.remaining();
                    let len = std::cmp::min(temp_buf.len(), remaining);
                    buf.put_slice(&temp_buf[..len]);
                    if temp_buf.len() > remaining {
                        temp_buf.drain(0..len);
                        self.read_fut = ReadFut::RemainingData(temp_buf);
                    } else {
                        self.read_fut = ReadFut::Idle;
                    }
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl AsyncWrite for PollStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        if let Some(fut) = self.write_fut.as_mut() {
            match fut.as_mut().poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(e)) => {
                    let stream = self.stream.clone();
                    let bytes = Bytes::copy_from_slice(buf);
                    self.write_fut = Some(Box::pin(async move { stream.write(&bytes).await }));
                    Poll::Ready(Err(e.into()))
                }
                // Given the data is buffered, it's okay to ignore the number of written bytes.
                //
                // TODO: In the long term, `stream.write` should be made sync. Then we could
                // remove the whole `if` condition and just call `stream.write`.
                Poll::Ready(Ok(_)) => {
                    let stream = self.stream.clone();
                    let bytes = Bytes::copy_from_slice(buf);
                    self.write_fut = Some(Box::pin(async move { stream.write(&bytes).await }));
                    Poll::Ready(Ok(buf.len()))
                }
            }
        } else {
            let stream = self.stream.clone();
            let bytes = Bytes::copy_from_slice(buf);
            let fut = self
                .write_fut
                .insert(Box::pin(async move { stream.write(&bytes).await }));

            match fut.as_mut().poll(cx) {
                // If it's the first time we're polling the future, `Poll::Pending` can't be
                // returned because that would mean the `PollStream` is not ready for writing. And
                // this is not true since we've just created a future, which is going to write the
                // buf to the underlying stream.
                //
                // It's okay to return `Poll::Ready` if the data is buffered (this is what the
                // buffered writer and `File` do).
                Poll::Pending => Poll::Ready(Ok(buf.len())),
                Poll::Ready(Err(e)) => {
                    self.write_fut = None;
                    Poll::Ready(Err(e.into()))
                }
                Poll::Ready(Ok(n)) => {
                    self.write_fut = None;
                    Poll::Ready(Ok(n))
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.write_fut.as_mut() {
            Some(fut) => match fut.as_mut().poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(e)) => {
                    self.write_fut = None;
                    Poll::Ready(Err(e.into()))
                }
                Poll::Ready(Ok(_)) => {
                    self.write_fut = None;
                    Poll::Ready(Ok(()))
                }
            },
            None => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.as_mut().poll_flush(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(_) => {}
        }
        let fut = match self.shutdown_fut {
            ShutdownFut::Done => return Poll::Ready(Ok(())),
            ShutdownFut::Errored(ref err) => return Poll::Ready(Err(err.clone().into())),
            ShutdownFut::ShuttingDown(ref mut fut) => fut,
            ShutdownFut::Idle => {
                let stream = self.stream.clone();
                self.shutdown_fut = ShutdownFut::ShuttingDown(Box::pin(async move {
                    stream.shutdown(Shutdown::Write).await
                }));
                self.shutdown_fut.get_shutting_down_mut()
            }
        };

        match fut.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => {
                self.shutdown_fut = ShutdownFut::Errored(e.clone());
                Poll::Ready(Err(e.into()))
            }
            Poll::Ready(Ok(_)) => {
                self.shutdown_fut = ShutdownFut::Done;
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl Clone for PollStream {
    fn clone(&self) -> PollStream {
        PollStream::new(self.clone_inner())
    }
}

impl fmt::Debug for PollStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollStream")
            .field("stream", &self.stream)
            .field("read_buf_cap", &self.read_buf_cap)
            .finish()
    }
}

impl AsRef<Stream> for PollStream {
    fn as_ref(&self) -> &Stream {
        &self.stream
    }
}
