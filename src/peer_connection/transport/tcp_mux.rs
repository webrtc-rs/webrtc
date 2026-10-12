//! TCP multiplexing: many peer connections sharing one ICE-TCP listener.
//!
//! The TCP sibling of [`udp_mux`](crate::peer_connection::transport::udp_mux) — the same
//! deployment shape (one well-known port for every connection, Pion's `ICETCPMux`), for
//! networks that block UDP outright.
//!
//! The demultiplexing differs from UDP in kind, not in principle. UDP has no connections, so
//! the UDP mux re-examines *every datagram*. A TCP listener hands each peer a separate stream,
//! so the TCP mux dispatches **once per accepted stream**: it reads the first framed STUN
//! message off the wire — ICE-TCP always opens with a connectivity check, which carries
//! `USERNAME = "local-ufrag:remote-ufrag"` naming the receiving connection — and hands the
//! stream to the connection registered under that ufrag, with the peeked frame replayed so
//! the check itself is not lost. Everything after the first frame is an ordinary
//! point-to-point stream and needs no muxing.
//!
//! Only the passive (listening) side is multiplexed: active ICE-TCP connects are outbound
//! dials, which need no shared listener. As with the UDP mux, the driver learns a connection's
//! ICE credentials from its *local description* at each gathering phase and registers the
//! ufrag with the mux — an ICE restart re-registers under the rotated ufrag while the previous
//! one stays routable for a grace period, and application-pinned credentials are registered
//! eagerly at `build`.
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use webrtc::peer_connection::transport::tcp_mux::TCPMuxDefault;
//! # use webrtc::peer_connection::{PeerConnectionBuilder, PeerConnectionEventHandler};
//! # struct MyHandler;
//! # #[async_trait::async_trait]
//! # impl PeerConnectionEventHandler for MyHandler {}
//! # async fn example() -> webrtc::error::Result<()> {
//! let listener = std::net::TcpListener::bind("0.0.0.0:8443")?;
//! let mux = TCPMuxDefault::new_default_runtime(listener)?;
//!
//! let pc = PeerConnectionBuilder::<&'static str>::new()
//!     .with_handler(Arc::new(MyHandler))
//!     .with_tcp_mux(mux.clone())
//!     .build()
//!     .await?;
//! # Ok(())
//! # }
//! ```

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use futures::FutureExt;
use log::{trace, warn};
use rtc::stun::message::{Message as StunMessage, is_stun_message};

use super::udp_mux::{stun_username, validate_ufrag};
use crate::runtime::{AsyncTcpListener, AsyncTcpStream, Notify, Runtime, timeout};

/// How long an accepted stream may take to present its first STUN frame before the mux gives
/// up on it. Only the dispatch task parks on it — the accept loop never blocks — but the
/// stream itself is held open meanwhile, so some bound is needed against idle connectors.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);

/// Inbound streams queued per connection. Accepts arrive in bursts at connect time; the
/// queue rides out the gap between the mux's dispatch and the driver's next select pass.
const MUXED_CONN_CHANNEL_CAPACITY: usize = 64;

/// A connection's registration identity in a [`TCPMux`].
///
/// Opaque to callers; handed back by [`TCPMux::register_conn`] and passed to
/// [`TCPMux::add_ufrag`]/[`TCPMux::remove_conn`]. Keying removal by the handle — rather than
/// by ufrag — is what makes a driver's `Drop` remove exactly *its* registration even after
/// the ufrag has been reused by a newer connection on the same mux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TCPMuxConnHandle(u64);

/// A TCP listener shared by many peer connections, dispatching accepted streams by the ICE
/// ufrag in their first STUN frame.
///
/// Constructed once per process (or per listen port) and shared by every
/// [`PeerConnectionBuilder::with_tcp_mux`](crate::peer_connection::PeerConnectionBuilder::with_tcp_mux)
/// call. See the [module documentation](self) for how dispatch works.
///
/// The trait is the seam for custom muxes; [`TCPMuxDefault`] is the built-in implementation.
pub trait TCPMux: fmt::Debug + Send + Sync + 'static {
    /// The address the shared listener is bound to.
    fn local_addr(&self) -> io::Result<SocketAddr>;

    /// Registers a **new** connection and returns its identity and the handle its inbound
    /// streams arrive on.
    ///
    /// The registration is created without a routing entry: the driver adds the connection's
    /// ICE ufrag(s) afterwards via [`add_ufrag`](TCPMux::add_ufrag), once the credentials are
    /// known — the agent-generated pair is only visible in the local description after the
    /// first `set_local_description`, which is also the earliest moment an inbound stream can
    /// name the connection (the peer needs our SDP first).
    fn register_conn(&self) -> io::Result<(TCPMuxConnHandle, TcpMuxedConn)>;

    /// Adds `ufrag` to a registered connection — its first routing entry, or a second one
    /// alongside the previous during an ICE restart, so streams opening with either ufrag
    /// dispatch to it.
    ///
    /// Fails with [`io::ErrorKind::AlreadyExists`] when the ufrag is taken — two connections
    /// registered under one ufrag would be handed each other's streams, so the duplicate must
    /// be refused rather than silently merged — and with [`io::ErrorKind::InvalidInput`] for
    /// an unknown handle or a malformed ufrag.
    fn add_ufrag(&self, handle: TCPMuxConnHandle, ufrag: &str) -> io::Result<()>;

    /// Removes one ufrag from a connection (an ICE-restart grace period ending). The
    /// connection itself is left intact; only streams opening with that ufrag stop routing.
    fn remove_ufrag(&self, ufrag: &str);

    /// Deregisters the connection identified by `handle`. Its dispatched-stream channel is
    /// closed and all its ufrags stop routing.
    fn remove_conn(&self, handle: TCPMuxConnHandle);

    /// Closes the mux: every registered connection is dropped and the accept loop stops.
    fn close(&self);

    /// How long an ICE-restarted ufrag stays routable after it is rotated away. Matches
    /// [`UDPMux::ufrag_grace`](crate::peer_connection::transport::udp_mux::UDPMux::ufrag_grace).
    fn ufrag_grace(&self) -> Duration {
        Duration::from_secs(30)
    }
}

/// The built-in [`TCPMux`]: one listener, an accept loop, and a first-frame peek per accepted
/// stream.
///
/// Dropping the last handle closes the mux; [`TCPMux::close`] does the same explicitly.
pub struct TCPMuxDefault {
    local_addr: SocketAddr,
    /// Routing entries: ufrag → registration handle. Several ufrags may name one connection
    /// during an ICE-restart grace period.
    ufrags: Mutex<HashMap<String, TCPMuxConnHandle>>,
    /// Registration handle → the connection's dispatched-stream channel.
    conns: Mutex<HashMap<TCPMuxConnHandle, async_channel::Sender<Arc<dyn AsyncTcpStream>>>>,
    /// Monotonic source of [`TCPMuxConnHandle`] tokens.
    next_handle: AtomicU64,
    /// Kept to spawn the per-stream dispatch tasks off the accept loop.
    runtime: Arc<dyn Runtime>,
    /// Wakes the accept loop out of its parked accept on [`close`](TCPMux::close): the loop
    /// parks in `listener.accept()`, which no flag check can interrupt — so close notifies,
    /// and the loop also checks the flag before it starts listening.
    closed_notify: Arc<Notify>,
    closed: AtomicBool,
}

impl fmt::Debug for TCPMuxDefault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TCPMuxDefault")
            .field("local_addr", &self.local_addr)
            .field("closed", &self.closed.load(Ordering::Relaxed))
            .finish()
    }
}

impl Drop for TCPMuxDefault {
    fn drop(&mut self) {
        // Last-handle drop is an implicit close: hang up the conns (their receivers would
        // otherwise park on a channel nobody feeds), and wake the accept loop out of its
        // parked accept — it holds the listener, not the mux, so without the wake the port
        // stays bound. The flag matters as much as the notification: a loop that has not
        // created its listener yet must still observe the shutdown.
        self.closed.store(true, Ordering::Release);
        self.conns.get_mut().unwrap().clear();
        self.closed_notify.notify_waiters();
    }
}

impl TCPMuxDefault {
    /// Creates a mux over `listener` on the given runtime and starts its accept loop.
    pub fn new(
        runtime: Arc<dyn Runtime>,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<Self>> {
        let wrapped = runtime.wrap_tcp_listener(listener)?;
        Self::from_listener(runtime, wrapped)
    }

    /// Creates a mux over an already-wrapped listener.
    pub fn from_listener(
        runtime: Arc<dyn Runtime>,
        listener: Arc<dyn AsyncTcpListener>,
    ) -> io::Result<Arc<Self>> {
        let local_addr = listener.local_addr()?;
        let mux = Arc::new(Self {
            local_addr,
            ufrags: Mutex::new(HashMap::new()),
            conns: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(0),
            runtime: runtime.clone(),
            closed_notify: Arc::new(Notify::new()),
            closed: AtomicBool::new(false),
        });
        // The accept loop holds the mux weakly, so dropping the last handle ends the task.
        runtime.spawn(Box::pin(Self::accept_loop(Arc::downgrade(&mux), listener)));
        Ok(mux)
    }

    /// [`new`](Self::new) on the compiled-in default runtime.
    #[cfg(any(feature = "runtime-tokio", feature = "runtime-smol"))]
    pub fn new_default_runtime(listener: std::net::TcpListener) -> io::Result<Arc<Self>> {
        let runtime = crate::runtime::default_runtime()
            .ok_or_else(|| io::Error::other("no async runtime found"))?;
        Self::new(runtime, listener)
    }

    /// Accept streams until the mux goes away; each accepted stream gets its own dispatch task,
    /// so a peer that is slow to send its first frame never holds up the loop.
    async fn accept_loop(mux: Weak<Self>, listener: Arc<dyn AsyncTcpListener>) {
        let Some(this) = mux.upgrade() else { return };
        // Created once: the listener observes `close()` from here on (runtime::Notify
        // registers at listen() creation, so no notification after this point is missed).
        let mut closed = Box::pin(this.closed_notify.listen().fuse());
        if this.closed.load(Ordering::Acquire) {
            // close() ran before we started listening.
            return;
        }
        drop(this);

        loop {
            let accept = futures::select! {
                accept = listener.accept().fuse() => accept,
                // The mux was closed while the accept was parked.
                _ = closed.as_mut() => return,
            };
            let Some(this) = mux.upgrade() else { return };
            match accept {
                Ok((stream, _peer_addr)) => {
                    let runtime = Arc::clone(&this.runtime);
                    runtime.spawn(Box::pin(Self::dispatch(mux.clone(), stream)));
                }
                Err(err) => {
                    warn!("tcp_mux: accept failed: {err}");
                }
            }
        }
    }

    /// Route one accepted stream: read its first framed STUN message and hand it to the
    /// connection the USERNAME names, with the peeked frame replayed. Anything else — no
    /// frame, not STUN, no USERNAME, an unknown ufrag — gets the stream closed unread, since
    /// there is no connection it could belong to.
    async fn dispatch(mux: Weak<Self>, stream: Arc<dyn AsyncTcpStream>) {
        let Some(this) = mux.upgrade() else { return };
        let peeked = timeout(&*this.runtime, FIRST_FRAME_TIMEOUT, Self::peek(&stream)).await;
        let (frame, ufrag) = match peeked {
            Ok(Ok(first)) => first,
            Ok(Err(err)) => {
                trace!("tcp_mux: dropping stream without a STUN first frame: {err}");
                return;
            }
            Err(_) => {
                trace!(
                    "tcp_mux: dropping stream that sent no frame within {FIRST_FRAME_TIMEOUT:?}"
                );
                return;
            }
        };

        let conn = this
            .ufrags
            .lock()
            .unwrap()
            .get(&ufrag)
            .and_then(|handle| this.conns.lock().unwrap().get(handle).cloned());
        match conn {
            Some(sink) => {
                let stream: Arc<dyn AsyncTcpStream> =
                    Arc::new(PrefixedTcpStream::new(frame, stream));
                // overflow: dropped — a full queue means the connection is not accepting
                // streams; the peer's checks time out and it retries or gives up.
                if sink.try_send(stream).is_err() {
                    trace!("tcp_mux: connection {ufrag} is not taking streams");
                }
            }
            None => {
                trace!("tcp_mux: dropping stream for unregistered ufrag {ufrag:?}");
            }
        }
    }

    /// Read exactly one framed packet off `stream` and return it verbatim (framing prefix
    /// included) together with the ufrag its STUN USERNAME names.
    async fn peek(stream: &Arc<dyn AsyncTcpStream>) -> io::Result<(Vec<u8>, String)> {
        // ICE-TCP frames every packet with a two-byte big-endian length prefix (RFC 4571
        // framing, matching `tcp_framing::frame_packet` on the write side).
        let mut prefix = [0u8; 2];
        read_exact(stream, &mut prefix).await?;
        let len = u16::from_be_bytes(prefix) as usize;
        if !is_stun_message_len(len) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("first frame of {len} bytes cannot be a STUN message"),
            ));
        }
        let mut payload = vec![0u8; len];
        read_exact(stream, &mut payload).await?;

        if !is_stun_message(&payload) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "first frame is not a STUN message",
            ));
        }
        let mut message = StunMessage::new();
        message
            .unmarshal_binary(&payload)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let ufrag = stun_username(&message).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "first STUN message carries no USERNAME",
            )
        })?;

        let mut frame = Vec::with_capacity(2 + len);
        frame.extend_from_slice(&prefix);
        frame.extend_from_slice(&payload);
        Ok((frame, ufrag))
    }
}

/// A STUN message is at least a bare header and never close to the 16-bit frame ceiling.
fn is_stun_message_len(len: usize) -> bool {
    (20..=u16::MAX as usize - 2).contains(&len)
}

/// Read `buf` to the end: `AsyncTcpStream::read` is allowed to return short reads, and a
/// framing peek must not mistake one for a complete frame.
async fn read_exact(stream: &Arc<dyn AsyncTcpStream>, buf: &mut [u8]) -> io::Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = stream.read(&mut buf[filled..]).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "stream closed mid-frame",
            ));
        }
        filled += n;
    }
    Ok(())
}

impl TCPMux for TCPMuxDefault {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn register_conn(&self) -> io::Result<(TCPMuxConnHandle, TcpMuxedConn)> {
        let mut conns = self.conns.lock().unwrap();
        // The closed check must hold the lock: close() sets the flag *before* draining the
        // table, so a registration that checked the flag unlocked could slip in behind the
        // drain and sit in a dead mux, never dispatched to and never hung up.
        if self.closed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "tcp mux is closed",
            ));
        }

        let handle = TCPMuxConnHandle(self.next_handle.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = async_channel::bounded(MUXED_CONN_CHANNEL_CAPACITY);
        conns.insert(handle, tx);
        Ok((handle, TcpMuxedConn { rx }))
    }

    fn add_ufrag(&self, handle: TCPMuxConnHandle, ufrag: &str) -> io::Result<()> {
        validate_ufrag(ufrag)?;
        let mut ufrags = self.ufrags.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "tcp mux is closed",
            ));
        }
        if ufrags.contains_key(ufrag) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("ICE ufrag {ufrag:?} is already registered on the TCP mux"),
            ));
        }
        if !self.conns.lock().unwrap().contains_key(&handle) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unknown tcp mux connection handle",
            ));
        }
        ufrags.insert(ufrag.to_owned(), handle);
        Ok(())
    }

    fn remove_ufrag(&self, ufrag: &str) {
        self.ufrags.lock().unwrap().remove(ufrag);
    }

    fn remove_conn(&self, handle: TCPMuxConnHandle) {
        // Dropping the sender closes the channel, which is what the owning driver's
        // receive arm sees as the end of the stream.
        self.conns.lock().unwrap().remove(&handle);
        self.ufrags.lock().unwrap().retain(|_, h| *h != handle);
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.conns.lock().unwrap().clear();
        self.ufrags.lock().unwrap().clear();
        // The accept loop parks in the listener's accept; wake it so the task actually ends.
        self.closed_notify.notify_waiters();
    }
}

/// A connection's registration in a [`TCPMux`]: the stream of inbound TCP streams routed to
/// its ufrag, each already carrying its peeked first frame for replay.
///
/// Received streams arrive in accept order and end (`None`) when the connection is
/// deregistered or the mux is closed.
pub struct TcpMuxedConn {
    // The raw async-channel receiver, not `runtime::Receiver`: that wrapper keeps `&mut self`
    // receivers, and the driver's select loop needs to poll this shared-immutably.
    rx: async_channel::Receiver<Arc<dyn AsyncTcpStream>>,
}

impl fmt::Debug for TcpMuxedConn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpMuxedConn").finish_non_exhaustive()
    }
}

impl TcpMuxedConn {
    /// The next inbound stream routed to this connection, or `None` once the registration is
    /// gone for good.
    pub async fn recv(&self) -> Option<Arc<dyn AsyncTcpStream>> {
        self.rx.recv().await.ok()
    }
}

/// An accepted stream with its first frame already consumed by the mux's peek: reads replay
/// the peeked bytes before falling through to the socket, so the connection's frame decoder
/// sees the stream as if nobody had touched it.
struct PrefixedTcpStream {
    prefix: Mutex<VecDeque<u8>>,
    inner: Arc<dyn AsyncTcpStream>,
}

impl PrefixedTcpStream {
    fn new(frame: Vec<u8>, inner: Arc<dyn AsyncTcpStream>) -> Self {
        Self {
            prefix: Mutex::new(frame.into()),
            inner,
        }
    }
}

impl fmt::Debug for PrefixedTcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrefixedTcpStream")
            .field("inner", &self.inner)
            .finish()
    }
}

impl AsyncTcpStream for PrefixedTcpStream {
    fn read<'a, 'b>(
        &'a self,
        buf: &'b mut [u8],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<usize>> + Send + 'b>>
    where
        'a: 'b,
    {
        Box::pin(async move {
            {
                let mut prefix = self.prefix.lock().unwrap();
                if !prefix.is_empty() {
                    let n = buf.len().min(prefix.len());
                    for (out, byte) in buf.iter_mut().zip(prefix.drain(..n)) {
                        *out = byte;
                    }
                    return Ok(n);
                }
            }
            self.inner.read(buf).await
        })
    }

    fn write_all<'a, 'b>(
        &'a self,
        buf: &'b [u8],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + 'b>>
    where
        'a: 'b,
    {
        self.inner.write_all(buf)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }
}

#[cfg(all(test, feature = "runtime-tokio"))]
mod tests {
    //! Framing-peek and dispatch tests over scripted fake streams; no sockets involved. The
    //! runtime is only needed because `dispatch` arms the first-frame read timeout on it.
    use super::*;
    use futures::FutureExt;
    use rtc::stun::attributes::ATTR_USERNAME;
    use rtc::stun::message::{BINDING_REQUEST, Message as StunMessage, TransactionId};
    use rtc::stun::textattrs::Username;

    /// A stream whose reads replay scripted chunks (to force short reads) and whose writes are
    /// recorded.
    struct FakeStream {
        chunks: Mutex<VecDeque<Vec<u8>>>,
        written: Mutex<Vec<u8>>,
    }

    impl fmt::Debug for FakeStream {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("FakeStream").finish()
        }
    }

    impl FakeStream {
        fn scripted(chunks: &[&[u8]]) -> Arc<Self> {
            Arc::new(Self {
                chunks: Mutex::new(chunks.iter().map(|c| c.to_vec()).collect()),
                written: Mutex::new(Vec::new()),
            })
        }
    }

    impl AsyncTcpStream for FakeStream {
        fn read<'a, 'b>(
            &'a self,
            buf: &'b mut [u8],
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<usize>> + Send + 'b>>
        where
            'a: 'b,
        {
            Box::pin(async move {
                let mut chunks = self.chunks.lock().unwrap();
                match chunks.pop_front() {
                    Some(chunk) => {
                        let n = chunk.len().min(buf.len());
                        buf[..n].copy_from_slice(&chunk[..n]);
                        if n < chunk.len() {
                            chunks.push_front(chunk[n..].to_vec());
                        }
                        Ok(n)
                    }
                    None => Ok(0), // EOF
                }
            })
        }

        fn write_all<'a, 'b>(
            &'a self,
            buf: &'b [u8],
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + 'b>>
        where
            'a: 'b,
        {
            self.written.lock().unwrap().extend_from_slice(buf);
            Box::pin(async { Ok(()) })
        }

        fn local_addr(&self) -> io::Result<SocketAddr> {
            Ok(SocketAddr::from(([127, 0, 0, 1], 8443)))
        }

        fn peer_addr(&self) -> io::Result<SocketAddr> {
            Ok(SocketAddr::from(([192, 0, 2, 1], 50000)))
        }
    }

    fn mux() -> Arc<TCPMuxDefault> {
        Arc::new(TCPMuxDefault {
            local_addr: SocketAddr::from(([127, 0, 0, 1], 8443)),
            ufrags: Mutex::new(HashMap::new()),
            conns: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(0),
            runtime: Arc::new(crate::runtime::TokioRuntime),
            closed_notify: Arc::new(Notify::new()),
            closed: AtomicBool::new(false),
        })
    }

    /// Register a connection and add its first ufrag, the way the driver does once it knows
    /// the connection's credentials.
    fn register(mux: &Arc<TCPMuxDefault>, ufrag: &str) -> (TCPMuxConnHandle, TcpMuxedConn) {
        let (handle, conn) = mux.register_conn().unwrap();
        mux.add_ufrag(handle, ufrag).unwrap();
        (handle, conn)
    }

    fn framed_binding_request(username: &str) -> Vec<u8> {
        let mut message = StunMessage::new();
        message
            .build(&[
                Box::new(BINDING_REQUEST),
                Box::<TransactionId>::default(),
                Box::new(Username::new(ATTR_USERNAME, username.to_owned())),
            ])
            .unwrap();
        let mut framed = (message.raw.len() as u16).to_be_bytes().to_vec();
        framed.extend_from_slice(&message.raw);
        framed
    }

    #[test]
    fn dispatch_routes_the_stream_by_the_first_frames_ufrag() {
        crate::runtime::TokioRuntime.block_on(Box::pin(async {
            let mux = mux();
            let (_, conn) = register(&mux, "aaaa");

            let frame = framed_binding_request("aaaa:remote");
            let stream: Arc<dyn AsyncTcpStream> = FakeStream::scripted(&[&frame]);
            TCPMuxDefault::dispatch(Arc::downgrade(&mux), stream).await;

            let routed = conn.recv().await.expect("stream routed to aaaa");
            // The peeked frame is replayed: the whole stream content is still there.
            let mut buf = vec![0u8; 4096];
            let n = routed.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], &frame[..]);
            assert_eq!(routed.read(&mut buf).await.unwrap(), 0, "then real EOF");
        }));
    }

    #[test]
    fn peek_survives_short_reads() {
        crate::runtime::TokioRuntime.block_on(Box::pin(async {
            let frame = framed_binding_request("aaaa:remote");
            // One byte at a time: the worst case for the framing read.
            let chunks: Vec<&[u8]> = frame.as_slice().chunks(1).collect();
            let stream: Arc<dyn AsyncTcpStream> = FakeStream::scripted(&chunks);
            let (got, ufrag) = TCPMuxDefault::peek(&stream).await.unwrap();
            assert_eq!(got, frame);
            assert_eq!(ufrag, "aaaa");
        }));
    }

    #[test]
    fn dispatch_drops_unroutable_streams() {
        crate::runtime::TokioRuntime.block_on(Box::pin(async {
            let mux = mux();
            let (_, conn) = register(&mux, "aaaa");

            // Unknown ufrag.
            let frame = framed_binding_request("zzzz:remote");
            TCPMuxDefault::dispatch(Arc::downgrade(&mux), FakeStream::scripted(&[&frame])).await;
            // Not STUN at all.
            TCPMuxDefault::dispatch(
                Arc::downgrade(&mux),
                FakeStream::scripted(&[b"\x00\x04junk"]),
            )
            .await;
            // Empty stream (EOF before a frame).
            TCPMuxDefault::dispatch(Arc::downgrade(&mux), FakeStream::scripted(&[])).await;

            assert!(conn.recv().now_or_never().is_none(), "nothing was routed");
        }));
    }

    #[test]
    fn add_ufrag_dispatches_under_both_ufrags_to_one_conn() {
        crate::runtime::TokioRuntime.block_on(Box::pin(async {
            let mux = mux();
            let (handle, conn) = register(&mux, "aaaa");

            // An ICE restart rotates the credentials; the old ufrag stays until its grace ends.
            mux.add_ufrag(handle, "bbbb").unwrap();

            for ufrag in ["aaaa", "bbbb"] {
                let frame = framed_binding_request(&format!("{ufrag}:remote"));
                TCPMuxDefault::dispatch(Arc::downgrade(&mux), FakeStream::scripted(&[&frame]))
                    .await;
                assert!(conn.recv().now_or_never().is_some(), "{ufrag} routes");
            }

            // Removing one ufrag stops that half only; the other still routes.
            mux.remove_ufrag("aaaa");
            let old_frame = framed_binding_request("aaaa:remote");
            TCPMuxDefault::dispatch(Arc::downgrade(&mux), FakeStream::scripted(&[&old_frame]))
                .await;
            assert!(
                conn.recv().now_or_never().is_none(),
                "removed ufrag no longer routes"
            );
            let new_frame = framed_binding_request("bbbb:remote");
            TCPMuxDefault::dispatch(Arc::downgrade(&mux), FakeStream::scripted(&[&new_frame]))
                .await;
            assert!(
                conn.recv().now_or_never().is_some(),
                "surviving ufrag still routes"
            );
        }));
    }

    #[test]
    fn remove_conn_by_handle_does_not_evict_a_reused_ufrag() {
        crate::runtime::TokioRuntime.block_on(Box::pin(async {
            let mux = mux();
            let (handle_a, conn_a) = register(&mux, "aaaa");

            // The old connection is deregistered (its driver's Drop) and its ufrag reused.
            mux.remove_conn(handle_a);
            let (_, conn_b) = register(&mux, "aaaa");

            // A belated removal of the *old* handle must not touch the new connection.
            mux.remove_conn(handle_a);

            let frame = framed_binding_request("aaaa:remote");
            TCPMuxDefault::dispatch(Arc::downgrade(&mux), FakeStream::scripted(&[&frame])).await;
            assert!(
                conn_b.recv().now_or_never().is_some(),
                "the reused ufrag still routes"
            );
            assert!(
                // now_or_never wraps the future's output: a closed channel completes with
                // `None`, so the stream check needs the outer layer peeled off.
                conn_a.recv().now_or_never().flatten().is_none(),
                "the old conn no longer routes"
            );
        }));
    }

    #[test]
    fn register_remove_and_close() {
        let mux = mux();
        let (handle, _) = register(&mux, "aaaa");
        let (handle_b, _) = mux.register_conn().unwrap();
        assert_eq!(
            mux.add_ufrag(handle_b, "aaaa").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists,
            "a duplicate ufrag is refused"
        );

        mux.remove_conn(handle);
        assert!(
            mux.add_ufrag(handle_b, "aaaa").is_ok(),
            "the ufrag is free again"
        );

        mux.close();
        assert!(
            mux.register_conn().is_err(),
            "a closed mux refuses registrations"
        );
    }
}
