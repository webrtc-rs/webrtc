use std::{
    fmt,
    future::Future,
    mem,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

use bytes::Bytes;
use futures_channel::{mpsc, oneshot};
use futures_util::{FutureExt, StreamExt};
use fxhash::FxHashMap;
use proto::{
    AssociationError, AssociationHandle, AssociationStats, ErrorCauseCode,
    PayloadProtocolIdentifier, StreamEvent, StreamId,
};
use thiserror::Error;
use tokio::time::{sleep_until, Instant as TokioInstant, Sleep};

use crate::{
    broadcast::{self, Broadcast},
    mutex::Mutex,
    send_stream::{SendStream, WriteError},
    AssociationEvent, EndpointEvent, RecvStream,
};

/// In-progress association attempt future
#[derive(Debug)]
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct Connecting {
    conn: Option<AssociationRef>,
    connected: oneshot::Receiver<()>,
    on_buffered_amount_low: Option<mpsc::UnboundedReceiver<StreamId>>,
}

impl Connecting {
    pub(crate) fn new(
        handle: AssociationHandle,
        conn: proto::Association,
        endpoint_events: mpsc::UnboundedSender<(AssociationHandle, EndpointEvent)>,
        conn_events: mpsc::UnboundedReceiver<AssociationEvent>,
        //udp_state: Arc<UdpState>,
    ) -> Connecting {
        let (on_buffered_amount_low_send, on_buffered_amount_low_recv) = mpsc::unbounded();
        let (on_connected_send, on_connected_recv) = oneshot::channel();
        let conn = AssociationRef::new(
            handle,
            conn,
            endpoint_events,
            conn_events,
            on_buffered_amount_low_send,
            on_connected_send,
            //udp_state,
        );

        tokio::spawn(AssociationDriver(conn.clone()));

        Connecting {
            conn: Some(conn),
            connected: on_connected_recv,
            on_buffered_amount_low: Some(on_buffered_amount_low_recv),
        }
    }

    /// The local IP address which was used when the peer established
    /// the association
    ///
    /// This can be different from the address the endpoint is bound to, in case
    /// the endpoint is bound to a wildcard address like `0.0.0.0` or `::`.
    ///
    /// This will return `None` for clients.
    ///
    /// Retrieving the local IP address is currently supported on the following
    /// platforms:
    /// - Linux
    ///
    /// On all non-supported platforms the local IP address will not be available,
    /// and the method will return `None`.
    pub fn local_ip(&self) -> Option<IpAddr> {
        let conn = self.conn.as_ref().unwrap();
        let inner = conn.lock("local_ip");

        inner.inner.local_ip()
    }
}

impl Future for Connecting {
    type Output = Result<NewAssociation, AssociationError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.connected.poll_unpin(cx).map(|_| {
            let conn = self.conn.take().unwrap();
            let inner = conn.lock("connecting");
            if inner.connected {
                drop(inner);
                Ok(NewAssociation::new(conn))
            } else {
                Err(inner
                    .error
                    .clone()
                    .expect("connected signaled without association success or error"))
            }
        })
    }
}

impl Connecting {
    /// The peer's UDP address.
    ///
    /// Will panic if called after `poll` has returned `Ready`.
    pub fn remote_addr(&self) -> SocketAddr {
        let conn_ref: &AssociationRef = self.conn.as_ref().expect("used after yielding Ready");
        conn_ref.lock("remote_addr").inner.remote_addr()
    }
}

/// Components of a newly established association
///
/// All fields of this struct, in addition to any other handles constructed later, must be dropped
/// for a association to be implicitly closed. If the `NewAssociation` is stored in a long-lived
/// variable, moving individual fields won't cause remaining unused fields to be dropped, even with
/// pattern-matching. The easiest way to ensure unused fields are dropped is to pattern-match on the
/// variable wrapped in brackets, which forces the entire `NewAssociation` to be moved out of the
/// variable and into a temporary, ensuring all unused fields are dropped at the end of the
/// statement:
///
#[cfg_attr(
    feature = "rustls",
    doc = "```rust
# use quinn::NewAssociation;
# fn dummy(new_association: NewAssociation) {
let NewAssociation { association, .. } = { new_association };
# }
```"
)]
///
/// You can also explicitly invoke [`Association::close()`] at any time.
///
/// [`Association::close()`]: crate::Association::close
#[derive(Debug)]
#[non_exhaustive]
pub struct NewAssociation {
    /// Handle for interacting with the association
    pub association: Association,
    /// Bidirectional streams initiated by the peer, in the order they were opened
    pub incoming_streams: IncomingStreams,
}

impl NewAssociation {
    fn new(conn: AssociationRef) -> Self {
        Self {
            association: Association(conn.clone()),
            incoming_streams: IncomingStreams(conn),
        }
    }
}

/// A future that drives protocol logic for a association
///
/// This future handles the protocol logic for a single association, routing events from the
/// `Association` API object to the `Endpoint` task and the related stream-related interfaces.
/// It also keeps track of outstanding timeouts for the `Association`.
///
/// If the association encounters an error condition, this future will yield an error. It will
/// terminate (yielding `Ok(())`) if the association was closed without error. Unlike other
/// association-related futures, this waits for the draining period to complete to ensure that
/// packets still in flight from the peer are handled gracefully.
#[must_use = "association drivers must be spawned for their associations to function"]
#[derive(Debug)]
struct AssociationDriver(AssociationRef);

impl Future for AssociationDriver {
    type Output = ();

    #[allow(unused_mut)] // MSRV
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let conn = &mut *self.0.lock("poll");

        if let Err(e) = conn.process_conn_events(cx) {
            conn.terminate(e);
            return Poll::Ready(());
        }
        let mut keep_going = conn.drive_transmit();
        // If a timer expires, there might be more to transmit. When we transmit something, we
        // might need to reset a timer. Hence, we must loop until neither happens.
        keep_going |= conn.drive_timer(cx);
        conn.forward_endpoint_events();
        conn.forward_app_events();

        if !conn.inner.is_drained() {
            if keep_going {
                // If the association hasn't processed all tasks, schedule it again
                cx.waker().wake_by_ref();
            } else {
                conn.driver = Some(cx.waker().clone());
            }
            return Poll::Pending;
        }
        if conn.error.is_none() {
            unreachable!("drained associations always have an error");
        }
        Poll::Ready(())
    }
}

/// A SCTP association.
///
/// If all references to a association (including every clone of the `Association` handle, streams of
/// incoming streams, and the various stream types) have been dropped, then the association will be
/// automatically closed with an `error_code` of 0 and an empty `reason`. You can also close the
/// association explicitly by calling [`Association::close()`].
///
/// May be cloned to obtain another handle to the same association.
///
/// [`Association::close()`]: Association::close
#[derive(Debug)]
pub struct Association(AssociationRef);

impl Association {
    /// Initiate a new outgoing bidirectional stream.
    ///
    /// Streams are cheap and instantaneous to open unless blocked by flow control. As a
    /// consequence, the peer won't be notified that a stream has been opened until the stream is
    /// actually used.
    pub fn open_stream(
        &self,
        stream_identifier: StreamId,
        default_payload_type: PayloadProtocolIdentifier,
    ) -> Opening {
        Opening {
            conn: self.0.clone(),
            state: broadcast::State::default(),
            stream_identifier,
            default_payload_type,
        }
    }

    /// Close the association immediately.
    ///
    /// Pending operations will fail immediately with [`AssociationError::LocallyClosed`]. Delivery
    /// of data on unfinished streams is not guaranteed, so the application must call this only
    /// when all important communications have been completed, e.g. by calling [`finish`] on
    /// outstanding [`Stream`]s and waiting for the resulting futures to complete.
    ///
    /// `error_code` and `reason` are not interpreted, and are provided directly to the peer.
    ///
    /// `reason` will be truncated to fit in a single packet with overhead; to improve odds that it
    /// is preserved in full, it should be kept under 1KiB.
    ///
    /// [`AssociationError::LocallyClosed`]: crate::AssociationError::LocallyClosed
    /// [`finish`]: crate::Stream::finish
    /// [`Stream`]: crate::Stream
    pub fn close(&self, error_code: ErrorCauseCode, reason: &[u8]) {
        let conn = &mut *self.0.lock("close");
        conn.close(error_code, Bytes::copy_from_slice(reason));
    }

    /// The peer's UDP address
    ///
    /// If `ServerConfig::migration` is `true`, clients may change addresses at will, e.g. when
    /// switching to a cellular internet association.
    pub fn remote_addr(&self) -> SocketAddr {
        self.0.lock("remote_addr").inner.remote_addr()
    }

    /// The local IP address which was used when the peer established
    /// the association
    ///
    /// This can be different from the address the endpoint is bound to, in case
    /// the endpoint is bound to a wildcard address like `0.0.0.0` or `::`.
    ///
    /// This will return `None` for clients.
    ///
    /// Retrieving the local IP address is currently supported on the following
    /// platforms:
    /// - Linux
    ///
    /// On all non-supported platforms the local IP address will not be available,
    /// and the method will return `None`.
    pub fn local_ip(&self) -> Option<IpAddr> {
        self.0.lock("local_ip").inner.local_ip()
    }

    /// Current best estimate of this association's latency (round-trip-time)
    pub fn rtt(&self) -> Duration {
        self.0.lock("rtt").inner.rtt()
    }

    /// Returns association statistics
    pub fn stats(&self) -> AssociationStats {
        self.0.lock("stats").inner.stats()
    }

    /// A stable identifier for this association
    ///
    /// Peer addresses and association IDs can change, but this value will remain
    /// fixed for the lifetime of the association.
    pub fn stable_id(&self) -> usize {
        self.0.stable_id()
    }
}

impl Clone for Association {
    fn clone(&self) -> Self {
        Association(self.0.clone())
    }
}

/// A stream of bidirectional SCTP streams initiated by a remote peer.
///
/// See `IncomingStreams` for information about incoming streams in general.
#[derive(Debug)]
pub struct IncomingStreams(AssociationRef);

impl futures_util::stream::Stream for IncomingStreams {
    type Item = Result<(SendStream, RecvStream), AssociationError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        //debug!("IncomingStreams::poll_next..");
        let mut conn = self.0.lock("IncomingStreams::poll_next");
        if let Some(x) = conn.inner.accept_stream() {
            let id = x.stream_identifier();
            //debug!("IncomingStreams::poll_next::accept_stream {}", id);
            conn.wake(); // To send additional stream ID credit
            mem::drop(conn); // Release the lock so clone can take it
            Poll::Ready(Some(Ok((
                SendStream::new(self.0.clone(), id),
                RecvStream::new(self.0.clone(), id),
            ))))
        } else if let Some(AssociationError::LocallyClosed) = conn.error {
            //debug!("IncomingStreams::poll_next::LocallyClosed");
            Poll::Ready(None)
        } else if let Some(ref e) = conn.error {
            //debug!("IncomingStreams::poll_next::error");
            Poll::Ready(Some(Err(e.clone())))
        } else {
            //debug!("IncomingStreams::poll_next::other");
            conn.incoming_streams_reader = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// A future that will resolve into an opened outgoing bidirectional stream
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct Opening {
    conn: AssociationRef,
    state: broadcast::State,
    stream_identifier: StreamId,
    default_payload_type: PayloadProtocolIdentifier,
}

impl Future for Opening {
    type Output = Result<(SendStream, RecvStream), AssociationError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut conn = this.conn.lock("Opening::next");
        if let Some(ref e) = conn.error {
            return Poll::Ready(Err(e.clone()));
        }
        if conn
            .inner
            .open_stream(this.stream_identifier, this.default_payload_type)
            .is_ok()
        {
            drop(conn); // Release lock for clone
            return Poll::Ready(Ok((
                SendStream::new(this.conn.clone(), this.stream_identifier),
                RecvStream::new(this.conn.clone(), this.stream_identifier),
            )));
        }
        conn.opening.register(cx, &mut this.state);
        Poll::Pending
    }
}

#[derive(Debug)]
pub struct AssociationRef(Arc<Mutex<AssociationInner>>);

impl AssociationRef {
    fn new(
        handle: AssociationHandle,
        conn: proto::Association,
        endpoint_events: mpsc::UnboundedSender<(AssociationHandle, EndpointEvent)>,
        conn_events: mpsc::UnboundedReceiver<AssociationEvent>,
        on_buffered_amount_low: mpsc::UnboundedSender<StreamId>,
        on_connected: oneshot::Sender<()>,
        //udp_state: Arc<UdpState>,
    ) -> Self {
        Self(Arc::new(Mutex::new(AssociationInner {
            inner: conn,
            driver: None,
            handle,
            on_buffered_amount_low: Some(on_buffered_amount_low),
            on_connected: Some(on_connected),
            connected: false,
            timer: None,
            timer_deadline: None,
            conn_events,
            endpoint_events,
            blocked_writers: FxHashMap::default(),
            blocked_readers: FxHashMap::default(),
            opening: Broadcast::new(),
            incoming_streams_reader: None,
            datagram_reader: None,
            finishing: FxHashMap::default(),
            stopped: FxHashMap::default(),
            error: None,
            ref_count: 0,
            //udp_state,
        })))
    }

    fn stable_id(&self) -> usize {
        &*self.0 as *const _ as usize
    }
}

impl Clone for AssociationRef {
    fn clone(&self) -> Self {
        self.lock("clone").ref_count += 1;
        Self(self.0.clone())
    }
}

impl Drop for AssociationRef {
    fn drop(&mut self) {
        let conn = &mut *self.lock("drop");
        if let Some(x) = conn.ref_count.checked_sub(1) {
            conn.ref_count = x;
            if x == 0 && !conn.inner.is_closed() {
                // If the driver is alive, it's just it and us, so we'd better shut it down. If it's
                // not, we can't do any harm. If there were any streams being opened, then either
                // the association will be closed for an unrelated reason or a fresh reference will
                // be constructed for the newly opened stream.
                conn.implicit_close();
            }
        }
    }
}

impl std::ops::Deref for AssociationRef {
    type Target = Mutex<AssociationInner>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct AssociationInner {
    pub(crate) inner: proto::Association,
    driver: Option<Waker>,
    handle: AssociationHandle,
    on_buffered_amount_low: Option<mpsc::UnboundedSender<StreamId>>,
    on_connected: Option<oneshot::Sender<()>>,
    connected: bool,
    timer: Option<Pin<Box<Sleep>>>,
    timer_deadline: Option<TokioInstant>,
    conn_events: mpsc::UnboundedReceiver<AssociationEvent>,
    endpoint_events: mpsc::UnboundedSender<(AssociationHandle, EndpointEvent)>,
    pub(crate) blocked_writers: FxHashMap<StreamId, Waker>,
    pub(crate) blocked_readers: FxHashMap<StreamId, Waker>,
    opening: Broadcast,
    incoming_streams_reader: Option<Waker>,
    datagram_reader: Option<Waker>,
    pub(crate) finishing: FxHashMap<StreamId, oneshot::Sender<Option<WriteError>>>,
    pub(crate) stopped: FxHashMap<StreamId, Waker>,
    /// Always set to Some before the association becomes drained
    pub(crate) error: Option<AssociationError>,
    /// Number of live handles that can be used to initiate or handle I/O; excludes the driver
    ref_count: usize,
}

impl AssociationInner {
    fn drive_transmit(&mut self) -> bool {
        let now = Instant::now();
        let mut transmits = 0;

        while let Some(t) = self.inner.poll_transmit(now) {
            transmits += match &t.payload {
                proto::Payload::RawEncode(s) => s.len(),
                _ => 0,
            };
            // If the endpoint driver is gone, noop.
            let _ = self
                .endpoint_events
                .unbounded_send((self.handle, EndpointEvent::Transmit(t)));

            if transmits >= MAX_TRANSMIT_DATAGRAMS {
                // TODO: What isn't ideal here yet is that if we don't poll all
                // datagrams that could be sent we don't go into the `app_limited`
                // state and CWND continues to grow until we get here the next time.
                // See https://github.com/quinn-rs/quinn/issues/1126
                return true;
            }
        }

        false
    }

    fn forward_endpoint_events(&mut self) {
        while let Some(event) = self.inner.poll_endpoint_event() {
            // If the endpoint driver is gone, noop.
            let _ = self
                .endpoint_events
                .unbounded_send((self.handle, EndpointEvent::Proto(event)));
        }
    }

    /// If this returns `Err`, the endpoint is dead, so the driver should exit immediately.
    fn process_conn_events(&mut self, cx: &mut Context<'_>) -> Result<(), AssociationError> {
        loop {
            match self.conn_events.poll_next_unpin(cx) {
                Poll::Ready(Some(AssociationEvent::Proto(event))) => {
                    self.inner.handle_event(event);
                }
                Poll::Ready(Some(AssociationEvent::Close { reason, error_code })) => {
                    self.close(error_code, reason);
                }
                Poll::Ready(None) => {
                    return Err(AssociationError::TransportError);
                }
                Poll::Pending => {
                    return Ok(());
                }
            }
        }
    }

    fn forward_app_events(&mut self) {
        while let Some(event) = self.inner.poll() {
            use proto::Event::*;
            match event {
                Connected => {
                    self.connected = true;
                    if let Some(x) = self.on_connected.take() {
                        // We don't care if the on-connected future was dropped
                        let _ = x.send(());
                    }
                }
                AssociationLost { reason } => {
                    self.terminate(reason);
                }
                Stream(StreamEvent::Writable { id }) => {
                    if let Some(writer) = self.blocked_writers.remove(&id) {
                        writer.wake();
                    }
                }
                Stream(StreamEvent::Opened) => {
                    if let Some(x) = self.incoming_streams_reader.take() {
                        x.wake();
                    }
                }
                DatagramReceived => {
                    if let Some(x) = self.datagram_reader.take() {
                        x.wake();
                    }
                }
                Stream(StreamEvent::Readable { id }) => {
                    if let Some(reader) = self.blocked_readers.remove(&id) {
                        reader.wake();
                    }
                }
                Stream(StreamEvent::Available) => {
                    let tasks = &mut self.opening;
                    tasks.wake();
                }
                Stream(StreamEvent::Finished { id }) => {
                    if let Some(finishing) = self.finishing.remove(&id) {
                        // If the finishing stream was already dropped, there's nothing more to do.
                        let _ = finishing.send(None);
                    }
                }
                Stream(StreamEvent::Stopped { id, error_code }) => {
                    if let Some(stopped) = self.stopped.remove(&id) {
                        stopped.wake();
                    }
                    if let Some(finishing) = self.finishing.remove(&id) {
                        let _ = finishing.send(Some(WriteError::Stopped(error_code)));
                    }
                    if let Some(writer) = self.blocked_writers.remove(&id) {
                        writer.wake();
                    }
                }
                Stream(StreamEvent::BufferedAmountLow { id }) => {
                    if let Some(x) = &self.on_buffered_amount_low {
                        let _ = x.unbounded_send(id);
                    }
                }
            }
        }
    }

    fn drive_timer(&mut self, cx: &mut Context<'_>) -> bool {
        // Check whether we need to (re)set the timer. If so, we must poll again to ensure the
        // timer is registered with the runtime (and check whether it's already
        // expired).
        match self.inner.poll_timeout().map(TokioInstant::from_std) {
            Some(deadline) => {
                if let Some(delay) = &mut self.timer {
                    // There is no need to reset the tokio timer if the deadline
                    // did not change
                    if self
                        .timer_deadline
                        .map(|current_deadline| current_deadline != deadline)
                        .unwrap_or(true)
                    {
                        delay.as_mut().reset(deadline);
                    }
                } else {
                    self.timer = Some(Box::pin(sleep_until(deadline)));
                }
                // Store the actual expiration time of the timer
                self.timer_deadline = Some(deadline);
            }
            None => {
                self.timer_deadline = None;
                return false;
            }
        }

        if self.timer_deadline.is_none() {
            return false;
        }

        let delay = self
            .timer
            .as_mut()
            .expect("timer must exist in this state")
            .as_mut();
        if delay.poll(cx).is_pending() {
            // Since there wasn't a timeout event, there is nothing new
            // for the association to do
            return false;
        }

        // A timer expired, so the caller needs to check for
        // new transmits, which might cause new timers to be set.
        self.inner.handle_timeout(Instant::now());
        self.timer_deadline = None;
        true
    }

    /// Wake up a blocked `Driver` task to process I/O
    pub(crate) fn wake(&mut self) {
        if let Some(x) = self.driver.take() {
            x.wake();
        }
    }

    /// Used to wake up all blocked futures when the association becomes closed for any reason
    fn terminate(&mut self, reason: AssociationError) {
        self.error = Some(reason.clone());
        if let Some(x) = self.on_buffered_amount_low.take() {
            let _ = x.unbounded_send(StreamId::MAX);
        }
        for (_, writer) in self.blocked_writers.drain() {
            writer.wake()
        }
        for (_, reader) in self.blocked_readers.drain() {
            reader.wake()
        }
        self.opening.wake();
        if let Some(x) = self.incoming_streams_reader.take() {
            x.wake();
        }
        if let Some(x) = self.datagram_reader.take() {
            x.wake();
        }
        for (_, x) in self.finishing.drain() {
            let _ = x.send(Some(WriteError::AssociationLost(reason.clone())));
        }
        if let Some(x) = self.on_connected.take() {
            let _ = x.send(());
        }
        for (_, waker) in self.stopped.drain() {
            waker.wake();
        }
    }

    fn close(&mut self, _error_code: ErrorCauseCode, _reason: Bytes) {
        let _ = self.inner.close(); //TODO: Instant::now(), error_code, reason);
        self.terminate(AssociationError::LocallyClosed);
        self.wake();
    }

    /// Close for a reason other than the application's explicit request
    pub fn implicit_close(&mut self) {
        self.close(0u16.into(), Bytes::new());
    }
}

impl Drop for AssociationInner {
    fn drop(&mut self) {
        if !self.inner.is_drained() {
            // Ensure the endpoint can tidy up
            let _ = self.endpoint_events.unbounded_send((
                self.handle,
                EndpointEvent::Proto(proto::EndpointEvent::drained()),
            ));
        }
    }
}
impl fmt::Debug for AssociationInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssociationInner")
            .field("inner", &self.inner)
            .finish()
    }
}

/// Errors that can arise when sending a datagram
#[derive(Debug, Error, Eq, Clone, PartialEq)]
pub enum SendDatagramError {
    /// The peer does not support receiving datagram frames
    #[error("datagrams not supported by peer")]
    UnsupportedByPeer,
    /// Datagram support is disabled locally
    #[error("datagram support disabled")]
    Disabled,
    /// The datagram is larger than the association can currently accommodate
    ///
    /// Indicates that the path MTU minus overhead or the limit advertised by the peer has been
    /// exceeded.
    #[error("datagram too large")]
    TooLarge,
    /// The association was lost
    #[error("association lost")]
    AssociationLost(#[from] AssociationError),
}

/// The maximum amount of datagrams which will be produced in a single `drive_transmit` call
///
/// This limits the amount of CPU resources consumed by datagram generation,
/// and allows other tasks (like receiving ACKs) to run in between.
const MAX_TRANSMIT_DATAGRAMS: usize = 20;
