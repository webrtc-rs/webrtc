//! Async Runtime Abstraction
//!
//! This module provides the [`Runtime`] trait, which abstracts every asynchronous
//! operation the WebRTC stack needs from its host runtime. Implement it to plug in any
//! async runtime and pass it per connection via
//! [`with_runtime`](crate::peer_connection::PeerConnectionBuilder::with_runtime).
//!
//! # What is injected, and what is not
//!
//! The abstraction is partitioned by a single question: *does this touch the reactor?*
//!
//! * **Reactor-bound** — task spawning, timers, UDP/TCP sockets, DNS, `block_on`. These
//!   go through [`Runtime`], because only the host runtime can drive them.
//! * **Executor-agnostic** — channels, broadcast, mutexes, notifications. These are plain
//!   waker-driven data structures that work anywhere, so they live in
//!   [`primitives`] with one implementation and are *not* feature-gated.
//!
//! Keeping the second group out of [`Runtime`] is what keeps the trait object-safe: a
//! generic method like `fn channel<T>(&self, …)` could not be called through
//! `dyn Runtime`.
//!
//! # Built-in runtimes
//!
//! Cargo features make built-in implementations available. They are **purely additive** —
//! a feature only decides whether a type exists, never which primitives the library uses:
//!
//! * **`runtime-tokio` (default)**: [`TokioRuntime`]
//! * **`runtime-smol`**: [`SmolRuntime`]
//! * **`runtime-mock`**: [`MockRuntime`](mock::MockRuntime), a deterministic virtual-clock
//!   runtime for tests.
//!
//! Enabling several at once is safe, and one process may drive different connections on
//! different runtimes. [`default_runtime`] returns the compiled-in default for callers
//! with no preference; it is a factory, not a settable registry.
//!
//! # Performance note
//!
//! [`Runtime`]'s async methods return boxed futures, so each call costs one allocation.
//! That is negligible for spawning, timers and connection setup.
//!
//! The packet path is different, so [`AsyncUdpSocket`] is **poll-based**:
//! [`poll_send`](AsyncUdpSocket::poll_send) and [`poll_recv`](AsyncUdpSocket::poll_recv)
//! are synchronous and readiness-driven, mirroring `quinn`'s socket trait. Callers on the
//! hot path (the peer-connection driver) poll them directly and allocate nothing per
//! datagram; the `async` methods on the trait are conveniences layered over them for
//! control-plane use. Use [`poll_once`] to probe readiness without allocating.

#![allow(clippy::type_complexity)]

use std::task::{Context, Poll};
use std::{fmt::Debug, future::Future, io, net::SocketAddr, pin::Pin, sync::Arc, time::Duration};

pub mod primitives;

pub use primitives::{
    BroadcastReceiver, BroadcastRecvError, BroadcastSendError, BroadcastSender, Mutex, Notify,
    Receiver, SendError, Sender, TryRecvError, TrySendError, UdpBatchState, UdpSockRef,
    broadcast_channel, channel,
};

pub(crate) const MAX_REACTOR_POOL_SIZE: usize = 1024;

/// A boxed, `Send` future — the return shape of the async [`Runtime`] methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A boxed future that need **not** be `Send`.
///
/// Used by [`Runtime::block_on`], which drives the future on the calling thread and so
/// never moves it across threads — matching `tokio::runtime::Runtime::block_on`,
/// `smol::block_on` and `futures::executor::block_on`, none of which require `Send`.
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// A handle to a task spawned via [`Runtime::spawn`].
///
/// Returned boxed, so a runtime supplies its own type without the crate wrapping it.
pub trait JoinHandle: Send + Sync {
    /// Let the task run to completion independently of this handle.
    ///
    /// Idempotent, and safe to call from `Drop` — which is where implementations wrapping a
    /// cancel-on-drop task should call it, so that dropping the handle detaches rather than
    /// cancelling.
    ///
    /// Also useful directly, to hand a task off without dropping the handle.
    fn detach(&self);

    /// Cancel the task cooperatively, at its next await point.
    fn abort(&self);

    /// Whether the task has run to completion (or been cancelled).
    fn is_finished(&self) -> bool;
}

/// Abstracts I/O and timer operations for runtime independence
///
/// This trait allows the WebRTC implementation to work with different async runtimes
/// without being tightly coupled to any specific runtime.
pub trait Runtime: Send + Sync + Debug + 'static {
    /// Drive a future to completion in the background
    ///
    /// The future must complete to `()` and will be spawned as a background task.
    /// Returns a handle that can be used to abort or inspect the task.
    /// Dropping the handle detaches the task; the task keeps running until it
    /// completes or the runtime is shut down. Call `.abort()` to cancel explicitly.
    #[track_caller]
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Box<dyn JoinHandle>;

    /// Drive `future` to completion on a **shared, bounded pool** of
    /// single-threaded reactors, pinned to one pool thread for its lifetime.
    ///
    /// The tokio and smol implementations keep a process-global pool of at most
    /// `reactor_pool_size` dedicated OS threads (each hosting its own single-threaded
    /// runtime), created lazily on first use and clamped to `1..=1024` — so the `0` default
    /// yields a single shared reactor thread. The pool is built once, so the size supplied by
    /// the first caller is the one that takes effect. Each `future` is
    /// assigned to one pool thread round-robin and never migrates off it, so the
    /// async runtime never moves a peer-connection driver across a shared worker
    /// pool — the dominant cost for in-process data-channel throughput (issue
    /// #101) — while the thread (and per-thread allocator arena) count stays
    /// bounded by `N` regardless of connection count, instead of one OS thread
    /// per connection. The socket wrapping and the whole event loop run inside
    /// `future`, on the pool thread's runtime, so I/O resources bind to it.
    ///
    /// `future` runs as an abortable *task* on its pool thread; the returned
    /// [`JoinHandle`] aborts that task (not a whole thread). Up to a few drivers
    /// cooperatively share a pool thread; they are I/O-bound and yield at await
    /// points, so they interleave without blocking one another.
    ///
    /// This is thread confinement, not CPU-core affinity: the OS scheduler may
    /// still move a pool thread between cores. Pinning pool threads to cores (via
    /// `core_affinity`) is a planned follow-up (issue #101).
    ///
    /// The default implementation falls back to [`Runtime::spawn`] on the ambient
    /// runtime, so custom runtimes keep working (without the confinement benefit).
    fn spawn_reactor(
        &self,
        _reactor_pool_size: usize,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn JoinHandle> {
        self.spawn(future)
    }

    /// Create an async UDP socket from a standard socket
    ///
    /// The socket should be bound and configured before being wrapped.
    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>>;

    /// Create an async TCP listener from a standard listener
    ///
    /// The listener should be bound and configured before being wrapped.
    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>>;

    /// Connect to a remote TCP address.
    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>>;

    /// Resolve a host string (`"host:port"`) to socket addresses.
    fn resolve_host<'a>(&'a self, host: &'a str) -> BoxFuture<'a, io::Result<Vec<SocketAddr>>>;

    /// Complete after `duration` has elapsed.
    ///
    /// Reactor-bound: only the host runtime can arm a timer, which is why this cannot be a
    /// free function.
    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;

    /// A repeating timer firing every `period`. The first tick fires immediately.
    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval>;

    /// Drive `future` to completion on this runtime, blocking the calling thread.
    ///
    /// The synchronous entry point for `main` and test harnesses. Restricted to a `()`
    /// output to stay object-safe — move a value out through a channel or `Arc<Mutex<_>>`.
    ///
    /// The future need not be `Send`: it is driven on the calling thread.
    ///
    /// # Panics
    ///
    /// Implementations may panic if called from inside their own executor.
    fn block_on(&self, future: LocalBoxFuture<'_, ()>);

    /// Cooperatively reschedule the current task so other ready tasks get a turn.
    ///
    /// The default wakes immediately and yields once. Override to integrate with a
    /// runtime's scheduling budget (e.g. tokio's cooperative yielding).
    fn yield_now(&self) -> BoxFuture<'static, ()> {
        let mut yielded = false;
        Box::pin(futures::future::poll_fn(move |cx| {
            if yielded {
                return std::task::Poll::Ready(());
            }
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }))
    }

    /// Short name for this runtime, used in log and error messages.
    fn name(&self) -> &'static str {
        "custom"
    }
}

/// A repeating timer, created by [`Runtime::interval`].
///
/// Object-safe: `tick` borrows `self` mutably and returns a boxed future.
pub trait AsyncInterval: Send + Sync {
    /// Wait until the next tick fires.
    fn tick(&mut self) -> BoxFuture<'_, ()>;
}

/// Returned by [`timeout`] when the deadline expires before the future completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elapsed;

impl std::fmt::Display for Elapsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "deadline has elapsed")
    }
}

impl std::error::Error for Elapsed {}

/// Run `future`, cancelling it if `duration` elapses first.
///
/// Derived generically from [`Runtime::sleep`], so every runtime gets it with no
/// per-runtime implementation.
pub async fn timeout<T>(
    runtime: &dyn Runtime,
    duration: Duration,
    future: impl Future<Output = T>,
) -> Result<T, Elapsed> {
    use futures::future::{Either, select};
    match select(Box::pin(future), runtime.sleep(duration)).await {
        Either::Left((value, _)) => Ok(value),
        Either::Right(_) => Err(Elapsed),
    }
}

/// Outcome of a batched UDP receive ([`AsyncUdpSocket::recv_gro`]).
///
/// `buf[..len]` holds one or more datagrams received in a single syscall. When the
/// kernel coalesced consecutive same-flow datagrams via UDP GRO, each is `stride`
/// bytes except possibly the last — walk `buf[..len]` in `stride`-sized steps to
/// recover the individual datagrams. Without GRO (or for a lone datagram)
/// `stride == len` and there is exactly one datagram. Every datagram in the batch
/// shares `peer_addr` (GRO only coalesces a single source flow).
#[derive(Debug, Clone, Copy)]
pub struct GroRecv {
    /// Total bytes written to the buffer across all coalesced datagrams.
    pub len: usize,
    /// Size of each datagram in the batch; the final one may be shorter. Always
    /// `>= 1` when `len > 0`.
    pub stride: usize,
    /// Source address shared by every datagram in the batch.
    pub peer_addr: SocketAddr,
}

/// Abstract implementation of a UDP socket for runtime independence.
///
/// # Poll-based, by design
///
/// The two primitives — [`poll_send`](Self::poll_send) and [`poll_recv`](Self::poll_recv) —
/// are **synchronous and readiness-based**, mirroring `quinn`'s `AsyncUdpSocket`. This is
/// what keeps the packet path allocation-free: a boxed future per datagram would cost one
/// heap allocation per send and per receive, and callers that merely want to *test*
/// readiness (see the driver's burst drain) would allocate just to discard.
///
/// The four `async` methods are conveniences with defaults written over the poll
/// primitives, so an implementor supplies two methods and gets all six.
pub trait AsyncUdpSocket: Send + Sync + Debug + 'static {
    /// Get the local address this socket is bound to
    fn local_addr(&self) -> io::Result<SocketAddr>;

    /// Attempt to send `buf` to `target`, registering `cx`'s waker if the socket is not
    /// writable yet.
    ///
    /// `segment_size` of `0` sends `buf` as a single datagram. A non-zero value requests
    /// UDP GSO: `buf` is split into consecutive datagrams of that size (the last may be
    /// shorter) and emitted in one syscall. Callers should only pass a `buf` spanning more
    /// than one segment when [`max_gso_segments`](Self::max_gso_segments) reports `> 1`.
    ///
    /// `ecn`, when `Some`, stamps the ECN codepoint bits on every segment.
    ///
    /// Returns the number of payload bytes accepted.
    fn poll_send(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
        segment_size: usize,
        target: SocketAddr,
        ecn: Option<u8>,
    ) -> Poll<io::Result<usize>>;

    /// Attempt to receive into `buf`, registering `cx`'s waker if no datagram is ready.
    ///
    /// May return several datagrams coalesced by UDP GRO — see [`GroRecv`] for how to
    /// split them back apart.
    fn poll_recv(&self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<GroRecv>>;

    /// Maximum number of segments a single [`send_segments`](Self::send_segments)
    /// call can emit in one syscall via UDP GSO. Returns `1` when GSO is
    /// unavailable (each segment then costs one syscall).
    fn max_gso_segments(&self) -> usize {
        1
    }

    /// Maximum number of datagrams the kernel may coalesce into one
    /// [`recv_gro`](Self::recv_gro) via UDP GRO. Returns `1` when GRO is
    /// unavailable. Used to size receive buffers.
    fn max_gro_segments(&self) -> usize {
        1
    }

    /// Send `buf` as a single datagram to `target`.
    fn send_to<'a>(
        &'a self,
        buf: &'a [u8],
        target: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        Box::pin(futures::future::poll_fn(move |cx| {
            self.poll_send(cx, buf, 0, target, None)
        }))
    }

    /// Receive a single datagram from the socket.
    fn recv_from<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<(usize, SocketAddr)>> + Send + 'a>> {
        Box::pin(async move {
            let gro = futures::future::poll_fn(|cx| self.poll_recv(cx, buf)).await?;
            Ok((gro.len, gro.peer_addr))
        })
    }

    /// Send `buf` as consecutive datagrams of `segment_size` bytes to `target`
    /// using a single UDP GSO (`UDP_SEGMENT`) syscall — the final datagram may be
    /// shorter than `segment_size`. `ecn`, when `Some`, stamps the ECN codepoint
    /// bits on every segment. Returns the number of payload bytes accepted.
    ///
    /// When the socket reports no GSO capability
    /// ([`max_gso_segments`](Self::max_gso_segments) `== 1`) but `buf` spans more than one
    /// segment, this splits into individual datagrams rather than emitting one oversized
    /// one — so an implementation that ignores `segment_size` still produces the correct
    /// wire format. Callers should nonetheless only batch when GSO is available.
    fn send_segments<'a>(
        &'a self,
        buf: &'a [u8],
        segment_size: usize,
        target: SocketAddr,
        ecn: Option<u8>,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        let needs_split =
            segment_size != 0 && buf.len() > segment_size && self.max_gso_segments() <= 1;
        if !needs_split {
            return Box::pin(futures::future::poll_fn(move |cx| {
                self.poll_send(cx, buf, segment_size, target, ecn)
            }));
        }
        Box::pin(async move {
            let mut sent = 0;
            for chunk in buf.chunks(segment_size) {
                sent += futures::future::poll_fn(|cx| self.poll_send(cx, chunk, 0, target, ecn))
                    .await?;
            }
            Ok(sent)
        })
    }

    /// Receive one or more datagrams into `buf` in a single syscall, using UDP GRO
    /// to coalesce consecutive same-flow datagrams when available. See [`GroRecv`]
    /// for how to split the buffer back into individual datagrams.
    fn recv_gro<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<GroRecv>> + Send + 'a>> {
        Box::pin(futures::future::poll_fn(move |cx| self.poll_recv(cx, buf)))
    }
}

/// Poll `f` once with a no-op waker, returning `None` if it is not ready.
///
/// For readiness probes on the poll-based socket primitives — draining datagrams that are
/// already queued without arming a wakeup, and **without allocating**. The previous
/// `recv_gro(..).now_or_never()` idiom boxed a future on every probe, including the common
/// case where the answer was "nothing ready".
pub fn poll_once<T>(f: impl FnOnce(&mut Context<'_>) -> Poll<T>) -> Option<T> {
    let waker = std::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    match f(&mut cx) {
        Poll::Ready(v) => Some(v),
        Poll::Pending => None,
    }
}

/// Abstract implementation of a TCP listener for runtime independence.
pub trait AsyncTcpListener: Send + Sync + Debug + 'static {
    /// Accept a new TCP stream.
    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = io::Result<(Arc<dyn AsyncTcpStream>, SocketAddr)>> + Send + 'a>>;

    /// Get the local address this listener is bound to.
    fn local_addr(&self) -> io::Result<SocketAddr>;
}

/// Abstract implementation of a TCP stream for runtime independence.
pub trait AsyncTcpStream: Send + Sync + Debug + 'static {
    /// Read bytes from the stream.
    fn read<'a, 'b>(
        &'a self,
        buf: &'b mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'b>>
    where
        'a: 'b;

    /// Write all bytes to the stream.
    fn write_all<'a, 'b>(
        &'a self,
        buf: &'b [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'b>>
    where
        'a: 'b;

    /// Get the local address of the stream.
    fn local_addr(&self) -> io::Result<SocketAddr>;

    /// Get the peer address of the stream.
    fn peer_addr(&self) -> io::Result<SocketAddr>;
}

/// Construct the compiled-in default runtime.
///
/// Returns [`TokioRuntime`] when `runtime-tokio` is enabled, else [`SmolRuntime`] when
/// `runtime-smol` is enabled, else `None`.
///
/// This is a convenience for callers with no runtime preference — it is **not** a
/// registry, and there is no way to overwrite what it returns. To use a custom runtime,
/// pass it explicitly to
/// [`with_runtime`](crate::peer_connection::PeerConnectionBuilder::with_runtime); that
/// also allows different connections in one process to use different runtimes.
pub fn default_runtime() -> Option<Arc<dyn Runtime>> {
    #[cfg(feature = "runtime-tokio")]
    {
        Some(Arc::new(TokioRuntime))
    }
    #[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
    {
        Some(Arc::new(SmolRuntime))
    }
    #[cfg(not(any(feature = "runtime-tokio", feature = "runtime-smol")))]
    {
        None
    }
}

// ── Built-in runtime implementations ──────────────────────────────────────────
//
// These modules are additive: a feature only decides whether an implementation type
// exists. Nothing else in the crate is feature-gated, so enabling several is safe.

#[cfg(feature = "runtime-tokio")]
mod tokio;
#[cfg(feature = "runtime-tokio")]
pub use tokio::TokioRuntime;

#[cfg(feature = "runtime-smol")]
mod smol;
#[cfg(feature = "runtime-smol")]
pub use smol::SmolRuntime;

#[cfg(feature = "runtime-mock")]
pub mod mock;
#[cfg(feature = "runtime-mock")]
pub use mock::MockRuntime;

#[cfg(test)]
mod default_impl_tests {
    //! Cover the `AsyncUdpSocket` DEFAULT method bodies (`send_to` / `recv_from` /
    //! `send_segments` / `recv_gro` / the capability getters) plus [`poll_once`]. The
    //! concrete tokio/smol impls override the capabilities, so a minimal fake that
    //! implements only the two poll primitives is what exercises the defaults.
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct FakeUdp {
        sent: Mutex<Vec<Vec<u8>>>,
        to_recv: Mutex<Vec<u8>>,
        /// When true, report GSO capability so `send_segments` forwards instead of splitting.
        gso: bool,
        /// When true, never become ready — used to test `poll_once` on a pending socket.
        never_ready: bool,
    }

    impl AsyncUdpSocket for FakeUdp {
        fn local_addr(&self) -> io::Result<SocketAddr> {
            Ok("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        }

        fn max_gso_segments(&self) -> usize {
            if self.gso { 8 } else { 1 }
        }

        fn poll_send(
            &self,
            _cx: &mut Context<'_>,
            buf: &[u8],
            _segment_size: usize,
            _target: SocketAddr,
            _ecn: Option<u8>,
        ) -> Poll<io::Result<usize>> {
            if self.never_ready {
                return Poll::Pending;
            }
            self.sent.lock().unwrap().push(buf.to_vec());
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_recv(&self, _cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<GroRecv>> {
            if self.never_ready {
                return Poll::Pending;
            }
            let data = self.to_recv.lock().unwrap();
            let n = data.len().min(buf.len());
            buf[..n].copy_from_slice(&data[..n]);
            Poll::Ready(Ok(GroRecv {
                len: n,
                stride: n.max(1),
                peer_addr: "127.0.0.1:9".parse::<SocketAddr>().unwrap(),
            }))
        }
    }

    fn addr() -> SocketAddr {
        "127.0.0.1:5".parse::<SocketAddr>().unwrap()
    }

    #[test]
    fn default_caps_are_one() {
        let s = FakeUdp::default();
        assert_eq!(s.max_gso_segments(), 1);
        assert_eq!(s.max_gro_segments(), 1);
    }

    #[test]
    fn default_send_to_forwards_one_datagram() {
        let s = FakeUdp::default();
        let n = futures::executor::block_on(s.send_to(b"abcd", addr())).unwrap();
        assert_eq!(n, 4);
        assert_eq!(s.sent.lock().unwrap().as_slice(), &[b"abcd".to_vec()]);
    }

    #[test]
    fn default_recv_from_derives_from_poll_recv() {
        let s = FakeUdp::default();
        *s.to_recv.lock().unwrap() = vec![7, 7, 7];
        let mut buf = [0u8; 16];
        let (n, from) = futures::executor::block_on(s.recv_from(&mut buf)).unwrap();
        assert_eq!(n, 3);
        assert_eq!(from, "127.0.0.1:9".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn send_segments_splits_when_socket_lacks_gso() {
        // Without GSO, a multi-segment buffer must become individual datagrams rather than
        // one oversized send — otherwise the wire format would be wrong.
        let s = FakeUdp::default();
        let buf = [1u8, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4];
        let sent = futures::executor::block_on(s.send_segments(&buf, 3, addr(), None)).unwrap();
        assert_eq!(sent, 11);
        let calls = s.sent.lock().unwrap();
        assert_eq!(calls.len(), 4, "3,3,3,2");
        assert_eq!(calls[0], vec![1, 1, 1]);
        assert_eq!(calls[3], vec![4, 4]);
    }

    #[test]
    fn send_segments_forwards_whole_buffer_when_gso_available() {
        // With GSO the kernel does the segmenting, so it must be a single syscall.
        let s = FakeUdp {
            gso: true,
            ..Default::default()
        };
        let buf = [9u8; 11];
        let sent = futures::executor::block_on(s.send_segments(&buf, 3, addr(), None)).unwrap();
        assert_eq!(sent, 11);
        assert_eq!(
            s.sent.lock().unwrap().len(),
            1,
            "GSO-capable socket gets one batched send"
        );
    }

    #[test]
    fn send_segments_zero_size_is_one_datagram() {
        let s = FakeUdp::default();
        let buf = [7u8; 10];
        futures::executor::block_on(s.send_segments(&buf, 0, addr(), Some(2))).unwrap();
        let calls = s.sent.lock().unwrap();
        assert_eq!(calls.len(), 1, "segment_size 0 must not shred the buffer");
        assert_eq!(calls[0].len(), 10);
    }

    #[test]
    fn default_recv_gro_reports_single_datagram_stride() {
        let s = FakeUdp::default();
        *s.to_recv.lock().unwrap() = vec![9, 9, 9, 9, 9];
        let mut buf = [0u8; 32];
        let gro = futures::executor::block_on(s.recv_gro(&mut buf)).unwrap();
        assert_eq!(gro.len, 5);
        assert_eq!(gro.stride, 5, "stride == len for a non-GRO datagram");
    }

    #[test]
    fn poll_once_probes_without_blocking_or_allocating() {
        let ready = FakeUdp::default();
        let mut buf = [0u8; 8];
        assert!(
            poll_once(|cx| ready.poll_recv(cx, &mut buf)).is_some(),
            "ready socket yields a value"
        );

        let pending = FakeUdp {
            never_ready: true,
            ..Default::default()
        };
        assert!(
            poll_once(|cx| pending.poll_recv(cx, &mut buf)).is_none(),
            "pending socket yields None instead of parking"
        );
    }
}
