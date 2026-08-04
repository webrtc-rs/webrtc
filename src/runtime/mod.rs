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
//! * **`runtime-tokio` (default)**: `TokioRuntime`
//! * **`runtime-smol`**: `SmolRuntime`
//! * **`runtime-mock`**: `MockRuntime`, a deterministic virtual-clock runtime for tests.
//!
//! Each type exists only when its feature is enabled, which is why they are named here
//! rather than linked — a link would dangle in any build without that feature.
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
//! datagram — including the batched send and receive paths, which have no boxed-future
//! wrappers at all. The two `async` socket methods are single-datagram conveniences for
//! control-plane use.

#![allow(clippy::type_complexity)]

use std::io::IoSliceMut;
use std::task::{Context, Poll};
use std::{fmt::Debug, future::Future, io, net::SocketAddr, pin::Pin, sync::Arc, time::Duration};

pub mod primitives;

pub use primitives::{
    BATCH_SIZE, BroadcastReceiver, BroadcastRecvError, BroadcastSendError, BroadcastSender,
    EcnCodepoint, Mutex, Notify, Receiver, RecvMeta, SendError, Sender, Transmit, TryRecvError,
    TrySendError, UdpSockRef, UdpSocketState, broadcast_channel, channel,
};

pub(crate) const MAX_REACTOR_POOL_SIZE: usize = 1024;

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
    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>>;

    /// Complete after `duration` has elapsed.
    ///
    /// Reactor-bound: only the host runtime can arm a timer, which is why this cannot be a
    /// free function.
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

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
    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>);

    /// Cooperatively reschedule the current task so other ready tasks get a turn.
    ///
    /// The default wakes immediately and yields once. Override to integrate with a
    /// runtime's scheduling budget (e.g. tokio's cooperative yielding).
    fn yield_now(&self) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
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
    fn tick(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
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

/// Abstract implementation of a UDP socket for runtime independence.
///
/// # Poll-based, by design
///
/// The two packet primitives — [`poll_send`](Self::poll_send) and
/// [`poll_recv`](Self::poll_recv) — are **synchronous and readiness-based**, mirroring
/// `quinn`'s `AsyncUdpSocket`. This is what keeps the packet path allocation-free: a boxed
/// future per datagram would cost one heap allocation per send and per receive, and callers
/// that merely want to *test* readiness (see the driver's burst drain) would allocate just
/// to discard.
///
/// An implementor supplies [`local_addr`](Self::local_addr) and the two poll primitives;
/// everything else is defaulted. The two `async` methods ([`send_to`](Self::send_to),
/// [`recv_from`](Self::recv_from)) are single-datagram conveniences for control-plane use,
/// written over the primitives — they each box a future, so the packet path polls instead.
/// Batching has no convenience wrappers by design: set `Transmit::segment_size` for
/// `poll_send`, and on the receive side pass more than one buffer to `poll_recv` and read
/// [`RecvMeta::stride`] from each message it fills.
pub trait AsyncUdpSocket: Send + Sync + Debug + 'static {
    /// Get the local address this socket is bound to
    fn local_addr(&self) -> io::Result<SocketAddr>;

    /// Attempt to send `transmit`, registering `cx`'s waker if the socket is not writable
    /// yet.
    ///
    /// [`contents`](Transmit::contents) goes to [`destination`](Transmit::destination) as a
    /// single datagram when [`segment_size`](Transmit::segment_size) is `None`. `Some(n)`
    /// requests UDP GSO: the buffer is split into consecutive `n`-byte datagrams (the last
    /// may be shorter) and emitted in one syscall. Callers should only supply contents
    /// spanning more than one segment when [`max_gso_segments`](Self::max_gso_segments)
    /// reports `> 1` — a socket reporting `1` may ignore `segment_size` and emit one
    /// oversized datagram.
    ///
    /// [`ecn`](Transmit::ecn), when `Some`, stamps the ECN codepoint bits on every segment.
    ///
    /// Returns the number of payload bytes accepted.
    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>>;

    /// Attempt to receive up to `bufs.len()` messages, registering `cx`'s waker if none is
    /// ready.
    ///
    /// Fills `bufs[..n]` and `meta[..n]` and returns `n`, never more than
    /// `bufs.len().min(meta.len())`. Two independent kinds of batching are in play:
    ///
    /// * **Multiple messages per syscall.** Where the platform offers `recvmmsg` (Linux) or
    ///   an equivalent, one call returns up to [`BATCH_SIZE`] datagrams *from different
    ///   peers*. Elsewhere `n` is always 1, so a caller must never assume more.
    /// * **GRO coalescing within a message.** Each filled buffer may itself hold several
    ///   consecutive same-flow datagrams — see [`RecvMeta::stride`].
    ///
    /// Implementations must report a `stride` of at least 1 for every filled message:
    /// `quinn-udp` mirrors `len` into `stride`, which is `0` for a zero-length datagram and
    /// would make de-segmentation divide by zero.
    ///
    /// A minimal implementation may fill only `bufs[0]` and return `Ok(1)`.
    ///
    /// # Errors
    ///
    /// Return the error rather than retrying inside the implementation: the driver
    /// classifies it and resumes the receive loop for anything transient. A socket serving
    /// many peers learns about per-peer failures through the socket itself, so
    /// `ConnectionRefused` (how an ICMP port-unreachable surfaces on Linux),
    /// `ConnectionReset` (the same on Windows), `Interrupted`, `WouldBlock` and `TimedOut`
    /// are all treated as transient and do not tear the socket down. Anything else is taken
    /// to mean the socket is unusable.
    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>>;

    /// Maximum number of segments a single [`poll_send`](Self::poll_send) call can emit in
    /// one syscall via UDP GSO. Returns `1` when GSO is unavailable (each segment then costs
    /// one syscall).
    ///
    /// Callers must consult this before passing a `segment_size` that spans more than one
    /// datagram: a socket reporting `1` may ignore `segment_size` and emit one oversized
    /// datagram.
    fn max_gso_segments(&self) -> usize {
        1
    }

    /// Maximum number of datagrams the kernel may coalesce into a **single message** via
    /// UDP GRO — not into a whole [`poll_recv`](Self::poll_recv) call, which may return up
    /// to [`BATCH_SIZE`] messages. Returns `1` when GRO is unavailable.
    ///
    /// # Buffer sizing
    ///
    /// This value decides how large a buffer the driver hands to
    /// [`poll_recv`](Self::poll_recv): reporting `n > 1` asks for roughly `n × 1500` bytes
    /// per message, because **a coalesced segment is bounded by the path MTU, not by the
    /// largest datagram the application sends**. Sizing against your own maximum datagram
    /// size instead is the easy mistake, and it fails quietly — the kernel drops the tail of
    /// a coalesced read, which looks like unexplained packet loss rather than an error.
    ///
    /// Two consequences worth knowing before reporting `> 1`:
    ///
    /// * Buffers are allocated per socket per receive, so the count is an allocation
    ///   multiplier. It is clamped internally, and `1500` is assumed per segment; paths with
    ///   an MTU above 1500 are not supported for GRO and would truncate.
    /// * The exact formula is the driver's own policy and may change. Report what the socket
    ///   can actually coalesce and let the driver size accordingly — an implementation that
    ///   allocates its own receive buffers to some other rule (a shared socket demultiplexed
    ///   across connections, say) is responsible for the same MTU bound in its own loop.
    fn max_gro_segments(&self) -> usize {
        1
    }

    /// Send `buf` as a single datagram to `target`.
    fn send_to<'a>(
        &'a self,
        buf: &'a [u8],
        target: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        let transmit = Transmit {
            destination: target,
            ecn: None,
            contents: buf,
            segment_size: None,
            src_ip: None,
        };
        Box::pin(futures::future::poll_fn(move |cx| {
            self.poll_send(cx, &transmit)
        }))
    }

    /// Receive a single datagram from the socket.
    ///
    /// Convenience over [`poll_recv`](Self::poll_recv) for control-plane callers wanting one
    /// datagram and no batching. It discards [`RecvMeta::stride`], so do not use it where
    /// GRO coalescing is possible — the packet path polls instead.
    fn recv_from<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<(usize, SocketAddr)>> + Send + 'a>> {
        Box::pin(async move {
            let mut meta = [RecvMeta::default(); 1];
            futures::future::poll_fn(|cx| {
                let mut bufs = [IoSliceMut::new(buf)];
                self.poll_recv(cx, &mut bufs, &mut meta)
            })
            .await?;
            Ok((meta[0].len, meta[0].addr))
        })
    }
}

/// Poll `f` once with a no-op waker, returning `None` if it is not ready.
///
/// For readiness probes on the poll-based socket primitives — draining datagrams that are
/// already queued without arming a wakeup, and **without allocating**. A
/// `poll_fn(..).now_or_never()` idiom would box a future on every probe, including the
/// common case where the answer is "nothing ready".
///
/// Crate-internal: it is a generic poll utility with nothing WebRTC- or runtime-specific
/// about it, and a runtime *implements* the socket traits rather than polling them, so
/// there is no reason to put it in the public API.
pub(crate) fn poll_once<T>(f: impl FnOnce(&mut Context<'_>) -> Poll<T>) -> Option<T> {
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
/// Returns `TokioRuntime` when `runtime-tokio` is enabled, else `SmolRuntime` when
/// `runtime-smol` is enabled, else `None`. (Named rather than linked: each type exists only
/// under its own feature.)
///
/// This is a convenience for callers with no runtime preference — it is **not** a
/// registry, and there is no way to overwrite what it returns. To use a custom runtime,
/// pass it explicitly to
/// [`with_runtime`](crate::peer_connection::PeerConnectionBuilder::with_runtime); that
/// also allows different connections in one process to use different runtimes.
pub fn default_runtime() -> Option<Arc<dyn Runtime>> {
    #[cfg(all(
        feature = "runtime-tokio",
        not(feature = "runtime-smol"),
        not(feature = "runtime-mock")
    ))]
    {
        Some(Arc::new(TokioRuntime))
    }
    #[cfg(all(
        not(feature = "runtime-tokio"),
        feature = "runtime-smol",
        not(feature = "runtime-mock")
    ))]
    {
        Some(Arc::new(SmolRuntime))
    }
    #[cfg(all(
        not(feature = "runtime-tokio"),
        not(feature = "runtime-smol"),
        feature = "runtime-mock"
    ))]
    {
        Some(Arc::new(MockRuntime::new()))
    }
    #[cfg(not(any(
        feature = "runtime-tokio",
        feature = "runtime-smol",
        feature = "runtime-mock"
    )))]
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
    //! Cover the `AsyncUdpSocket` DEFAULT method bodies (`send_to` / `recv_from` and the
    //! capability getters) plus [`poll_once`]. The concrete tokio/smol impls override the
    //! capabilities, so a minimal fake that implements only the two poll primitives is what
    //! exercises the defaults.
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct FakeUdp {
        sent: Mutex<Vec<Vec<u8>>>,
        to_recv: Mutex<Vec<u8>>,
        /// When true, never become ready — used to test `poll_once` on a pending socket.
        never_ready: bool,
    }

    impl AsyncUdpSocket for FakeUdp {
        fn local_addr(&self) -> io::Result<SocketAddr> {
            Ok("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        }

        fn poll_send(
            &self,
            _cx: &mut Context<'_>,
            transmit: &Transmit<'_>,
        ) -> Poll<io::Result<usize>> {
            if self.never_ready {
                return Poll::Pending;
            }
            self.sent.lock().unwrap().push(transmit.contents.to_vec());
            Poll::Ready(Ok(transmit.contents.len()))
        }

        fn poll_recv(
            &self,
            _cx: &mut Context<'_>,
            bufs: &mut [IoSliceMut<'_>],
            meta: &mut [RecvMeta],
        ) -> Poll<io::Result<usize>> {
            if self.never_ready {
                return Poll::Pending;
            }
            // Minimal implementation: one message per call, as a non-`recvmmsg` platform.
            let data = self.to_recv.lock().unwrap();
            let n = data.len().min(bufs[0].len());
            bufs[0][..n].copy_from_slice(&data[..n]);
            // `RecvMeta` is `#[non_exhaustive]`: assign fields rather than using a struct
            // literal, which is not permitted outside `quinn-udp`.
            meta[0] = RecvMeta::default();
            meta[0].len = n;
            meta[0].stride = n.max(1);
            meta[0].addr = "127.0.0.1:9".parse::<SocketAddr>().unwrap();
            Poll::Ready(Ok(1))
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
    fn poll_once_probes_without_blocking_or_allocating() {
        let ready = FakeUdp::default();
        let mut buf = [0u8; 8];
        let mut meta = [RecvMeta::default(); 1];
        assert!(
            poll_once(|cx| {
                let mut bufs = [IoSliceMut::new(&mut buf)];
                ready.poll_recv(cx, &mut bufs, &mut meta)
            })
            .is_some(),
            "ready socket yields a value"
        );

        let pending = FakeUdp {
            never_ready: true,
            ..Default::default()
        };
        assert!(
            poll_once(|cx| {
                let mut bufs = [IoSliceMut::new(&mut buf)];
                pending.poll_recv(cx, &mut bufs, &mut meta)
            })
            .is_none(),
            "pending socket yields None instead of parking"
        );
    }
}
