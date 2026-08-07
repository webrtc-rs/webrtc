//! Deterministic runtime for tests.
//!
//! [`MockRuntime`] drives timers from a [`VirtualClock`] instead of wall-clock time, so a
//! test can advance time instantly and deterministically:
//!
//! ```
//! # use std::sync::Arc;
//! # use std::time::{Duration, Instant};
//! # use webrtc::runtime::{Runtime, mock::MockRuntime};
//! let rt = Arc::new(MockRuntime::new());
//! let clock = rt.clock();
//!
//! let elapsed = Arc::new(std::sync::atomic::AtomicBool::new(false));
//! let flag = Arc::clone(&elapsed);
//! let sleep = rt.sleep(Duration::from_secs(30));
//! let task = async move {
//!     sleep.await;
//!     flag.store(true, std::sync::atomic::Ordering::SeqCst);
//! };
//! futures::pin_mut!(task);
//!
//! // Nothing fires until time is advanced.
//! assert!(futures::FutureExt::now_or_never(&mut task).is_none());
//! clock.advance(Duration::from_secs(30));
//! assert!(futures::FutureExt::now_or_never(&mut task).is_some());
//! assert!(elapsed.load(std::sync::atomic::Ordering::SeqCst));
//! ```
//!
//! Each `MockRuntime` owns an independent clock, so tests using one may run in parallel —
//! there is no process-global runtime state (see the crate's pluggable-runtime design).
//!
//! # Scope
//!
//! This runtime covers **timers, task execution and UDP**. [`MockUDPNetwork`] delivers datagrams
//! in memory, synchronously, so two peers sharing one can complete ICE, DTLS and SCTP without
//! a socket or a millisecond of wall-clock time — which is what makes an end-to-end test of
//! timing-dependent behaviour possible at all.
//!
//! TCP operations (`wrap_tcp_listener`, `connect_tcp`) still return
//! [`io::ErrorKind::Unsupported`]; ICE-TCP under the mock is a follow-up.

use super::{
    AsyncInterval, AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, RecvMeta, Runtime,
    Transmit,
};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io;
use std::io::IoSliceMut;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

/// A manually advanced clock. Timers registered against it fire only when
/// [`advance`](Self::advance) moves past their deadline.
#[derive(Debug)]
pub struct VirtualClock {
    state: Mutex<ClockState>,
    /// The real instant this clock was created at.
    ///
    /// [`Instant`] is opaque and cannot be constructed from nothing, so a virtual instant is
    /// this base plus the elapsed virtual [`Duration`]. Only differences between instants are
    /// meaningful, so where the base actually sits is irrelevant — what matters is that
    /// [`now_instant`](Self::now_instant) advances *only* when [`advance`](Self::advance) is
    /// called, never with the wall clock.
    base: Instant,
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self {
            state: ClockState::default().into(),
            base: Instant::now(),
        }
    }
}

#[derive(Debug, Default)]
struct ClockState {
    /// Time elapsed since the clock was created.
    time_elapsed: Duration,
    /// Pending timers, keyed by registration id.
    timers: HashMap<u64, Timer>,
    next_id: u64,
}

#[derive(Debug)]
struct Timer {
    deadline: Duration,
    waker: Option<Waker>,
}

impl VirtualClock {
    /// Create a clock positioned at time zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Time elapsed since this clock was created.
    pub fn elapsed(&self) -> Duration {
        self.state.lock().expect("clock poisoned").time_elapsed
    }

    /// The current virtual time as an [`Instant`], for handing to the sans-I/O core.
    ///
    /// This is what makes `clock.advance(..)` visible to protocol logic: the core is *told* the
    /// time through `handle_timeout(now)` and the timestamps on inbound messages, so a clock the
    /// driver reads is the only clock the core sees.
    pub fn now(&self) -> Instant {
        self.base + self.elapsed()
    }

    /// Advance the clock by `delta`, waking every timer whose deadline has passed.
    ///
    /// Wakers are invoked after the internal lock is released, so a woken task may
    /// register a new timer without deadlocking.
    pub fn advance(&self, delta: Duration) {
        let wakers = {
            let mut state = self.state.lock().expect("clock poisoned");
            state.time_elapsed += delta;
            let now = state.time_elapsed;
            let due: Vec<u64> = state
                .timers
                .iter()
                .filter(|(_, t)| t.deadline <= now)
                .map(|(id, _)| *id)
                .collect();
            due.into_iter()
                .filter_map(|id| state.timers.remove(&id).and_then(|t| t.waker))
                .collect::<Vec<_>>()
        };
        for waker in wakers {
            waker.wake();
        }
    }

    /// Number of timers still pending. Useful for asserting a test left none armed.
    pub fn pending_timers(&self) -> usize {
        self.state.lock().expect("clock poisoned").timers.len()
    }

    /// Register a timer `delay` from the current time, returning its id. A `delay` of zero
    /// is already due and yields `None`.
    fn register(&self, delay: Duration) -> Option<u64> {
        let mut state = self.state.lock().expect("clock poisoned");
        if delay.is_zero() {
            return None;
        }
        let deadline = state.time_elapsed + delay;
        let id = state.next_id;
        state.next_id += 1;
        state.timers.insert(
            id,
            Timer {
                deadline,
                waker: None,
            },
        );
        Some(id)
    }

    /// Poll a registered timer, parking `cx`'s waker if it has not yet fired.
    fn poll_timer(&self, id: u64, cx: &mut Context<'_>) -> Poll<()> {
        let mut state = self.state.lock().expect("clock poisoned");
        match state.timers.get_mut(&id) {
            // Removed by `advance` => fired.
            None => Poll::Ready(()),
            Some(timer) => {
                timer.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }

    /// Cancel a pending timer (used when a `Sleep` future is dropped before firing).
    fn cancel(&self, id: u64) {
        self.state
            .lock()
            .expect("clock poisoned")
            .timers
            .remove(&id);
    }
}

/// Future returned by [`MockRuntime::sleep`].
struct Sleep {
    clock: Arc<VirtualClock>,
    /// `None` once fired (or if the delay was zero).
    id: Option<u64>,
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        match self.id {
            None => Poll::Ready(()),
            Some(id) => match self.clock.poll_timer(id, cx) {
                Poll::Ready(()) => {
                    self.id = None;
                    Poll::Ready(())
                }
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            self.clock.cancel(id);
        }
    }
}

/// Repeating timer over a [`VirtualClock`].
struct MockInterval {
    clock: Arc<VirtualClock>,
    period: Duration,
    first: bool,
}

impl AsyncInterval for MockInterval {
    fn tick(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        // First tick fires immediately, matching the built-in runtimes.
        if self.first {
            self.first = false;
            return Box::pin(std::future::ready(()));
        }
        let id = self.clock.register(self.period);
        let clock = Arc::clone(&self.clock);
        Box::pin(Sleep { clock, id })
    }
}

/// Handle for a task spawned on a [`MockRuntime`].
struct MockJoinHandle {
    abort: futures::future::AbortHandle,
    finished: Arc<AtomicBool>,
}

// No `impl Drop` needed: each task owns a thread that keeps running once the handle goes
// away, which already satisfies the detach-on-drop contract.
impl super::JoinHandle for MockJoinHandle {
    fn detach(&self) {
        // Nothing to do — the task's thread is already independent of this handle.
    }

    fn abort(&self) {
        self.abort.abort();
    }

    fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

/// An in-memory UDP network shared by the sockets of one or more [`MockRuntime`]s.
///
/// Datagrams are delivered synchronously into the destination's inbox at `poll_send` time and
/// the receiver's waker is fired, so delivery costs no wall-clock time and needs no reactor.
/// That is what lets an end-to-end connection run entirely under a [`VirtualClock`]: nothing in
/// the path waits on the operating system.
///
/// Sending to an address nobody has bound is a silent drop, matching UDP.
#[derive(Debug, Default)]
pub struct MockUDPNetwork {
    inboxes: Mutex<HashMap<SocketAddr, Inbox>>,
}

#[derive(Debug, Default)]
struct Inbox {
    /// Datagrams waiting to be read, as `(source, payload)`.
    packets: VecDeque<(SocketAddr, Vec<u8>)>,
    /// Waker of a task parked in `poll_recv` on this address.
    waker: Option<Waker>,
}

impl MockUDPNetwork {
    /// Create an empty network with no sockets bound.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind `addr`, returning a socket that sends and receives on this network.
    ///
    /// # Errors
    ///
    /// Fails with [`io::ErrorKind::AddrInUse`] if `addr` is already bound.
    pub fn bind(self: &Arc<Self>, addr: SocketAddr) -> io::Result<Arc<MockUdpSocket>> {
        let mut inboxes = self.inboxes.lock().expect("network poisoned");
        if inboxes.contains_key(&addr) {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("mock network: {addr} is already bound"),
            ));
        }
        inboxes.insert(addr, Inbox::default());
        Ok(Arc::new(MockUdpSocket {
            local_addr: addr,
            udp_network: Arc::clone(self),
        }))
    }

    /// Deliver one datagram, waking a parked receiver. Unbound destinations are dropped.
    fn deliver(&self, from: SocketAddr, to: SocketAddr, payload: &[u8]) {
        let waker = {
            let mut inboxes = self.inboxes.lock().expect("network poisoned");
            let Some(inbox) = inboxes.get_mut(&to) else {
                return; // nothing bound: a UDP datagram into the void
            };
            inbox.packets.push_back((from, payload.to_vec()));
            inbox.waker.take()
        };
        // Wake outside the lock: the woken task may immediately re-register.
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// A UDP socket on a [`MockUDPNetwork`].
#[derive(Debug)]
pub struct MockUdpSocket {
    local_addr: SocketAddr,
    udp_network: Arc<MockUDPNetwork>,
}

impl Drop for MockUdpSocket {
    fn drop(&mut self) {
        // Match UDP semantics: dropping a socket releases its bound address.
        self.udp_network
            .inboxes
            .lock()
            .expect("udp network poisoned")
            .remove(&self.local_addr);
    }
}

impl AsyncUdpSocket for MockUdpSocket {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn poll_send(&self, _cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        // Always writable: there is no send buffer to fill.
        //
        // `segment_size` is honoured even though `max_gso_segments` reports 1, because
        // splitting here is trivial and keeps the mock honest if a caller sends a GSO batch
        // anyway — the alternative (one oversized datagram) would corrupt the stream.
        match transmit.segment_size {
            Some(size) if size > 0 => {
                for chunk in transmit.contents.chunks(size) {
                    self.udp_network
                        .deliver(self.local_addr, transmit.destination, chunk);
                }
            }
            _ => self
                .udp_network
                .deliver(self.local_addr, transmit.destination, transmit.contents),
        }
        Poll::Ready(Ok(transmit.contents.len()))
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let capacity = bufs.len().min(meta.len());
        if capacity == 0 {
            return Poll::Ready(Ok(0));
        }

        let mut inboxes = self.udp_network.inboxes.lock().expect("network poisoned");
        let inbox = inboxes.get_mut(&self.local_addr).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "mock socket is not bound")
        })?;

        if inbox.packets.is_empty() {
            // Park. The waker is fired by `deliver`, never by the clock: an idle socket must
            // not wake just because time advanced.
            inbox.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }

        let mut n = 0;
        while n < capacity {
            let Some((from, payload)) = inbox.packets.pop_front() else {
                break;
            };
            let len = payload.len().min(bufs[n].len());
            bufs[n][..len].copy_from_slice(&payload[..len]);
            // `RecvMeta` is `#[non_exhaustive]`: build from `default()` and assign.
            let mut m = RecvMeta::default();
            m.addr = from;
            m.len = len;
            // One datagram per message: no GRO coalescing here. Must be >= 1 even for a
            // zero-length datagram, or the driver's de-segmentation divides by zero.
            m.stride = len.max(1);
            m.dst_ip = Some(self.local_addr.ip());
            meta[n] = m;
            n += 1;
        }
        Poll::Ready(Ok(n))
    }
}

/// A deterministic [`Runtime`] backed by a [`VirtualClock`].
///
/// See the [module docs](self) for usage and scope.
#[derive(Debug, Default)]
pub struct MockRuntime {
    clock: Arc<VirtualClock>,
    udp_network: Arc<MockUDPNetwork>,
}

impl MockRuntime {
    /// Create a runtime with a fresh clock at time zero and a private network.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a runtime whose sockets live on `network`.
    ///
    /// Two peers can only exchange datagrams if they share one, so an end-to-end test builds
    /// the network first and hands the same handle to both runtimes. Each keeps its own clock:
    /// advancing one does not advance the other, which is deliberate — a test that wants the
    /// two peers to see the same time must advance both.
    pub fn with_network(network: Arc<MockUDPNetwork>) -> Self {
        Self {
            clock: Arc::new(VirtualClock::new()),
            udp_network: network,
        }
    }

    /// Handle to this runtime's clock, for advancing time in tests.
    pub fn clock(&self) -> Arc<VirtualClock> {
        Arc::clone(&self.clock)
    }

    /// Handle to this runtime's network, for binding further sockets or sharing it.
    pub fn network(&self) -> Arc<MockUDPNetwork> {
        Arc::clone(&self.udp_network)
    }
}

impl Runtime for MockRuntime {
    /// The virtual clock's current instant.
    ///
    /// This is the override that makes the whole thing work: `clock.advance(30s)` moves this,
    /// the driver passes it to `handle_timeout`, and the core's ICE consent / DTLS retransmit /
    /// SCTP RTO logic decides against it — with no wall-clock time having passed.
    fn now(&self) -> Instant {
        self.clock.now()
    }

    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Box<dyn JoinHandle> {
        // One OS thread per task, driven by a minimal executor. Adequate for tests and
        // avoids pulling a work-stealing scheduler into the mock; determinism comes from
        // the clock, which gates every timer.
        let (abortable, abort) = futures::future::abortable(future);
        let finished = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&finished);
        std::thread::spawn(move || {
            let _ = futures::executor::block_on(abortable);
            done.store(true, Ordering::SeqCst);
        });
        Box::new(MockJoinHandle { abort, finished })
    }

    /// Re-bind `socket`'s address on this runtime's in-memory network.
    ///
    /// The real socket is only consulted for the address it bound, then dropped: the caller
    /// binds a `std::net::UdpSocket` to get an ephemeral port allocated by the OS, and this
    /// takes that address into the mock network. Nothing is sent over the real socket, so no
    /// datagram leaves the process and no wall-clock time is spent waiting on one.
    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        let addr = socket.local_addr()?;
        drop(socket);
        Ok(self.udp_network.bind(addr)? as Arc<dyn AsyncUdpSocket>)
    }

    fn wrap_tcp_listener(
        &self,
        _listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>> {
        Err(unsupported("wrap_tcp_listener"))
    }

    fn connect_tcp<'a>(
        &'a self,
        _remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>> {
        Box::pin(async move { Err(unsupported("connect_tcp")) })
    }

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>> {
        // Accept literal addresses so tests can exercise ICE paths without DNS.
        Box::pin(async move {
            match host.parse::<SocketAddr>() {
                Ok(addr) => Ok(vec![addr]),
                Err(_) => Err(unsupported("resolve_host (non-literal address)")),
            }
        })
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let id = self.clock.register(duration);
        Box::pin(Sleep {
            clock: Arc::clone(&self.clock),
            id,
        })
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        Box::new(MockInterval {
            clock: Arc::clone(&self.clock),
            period,
            first: true,
        })
    }

    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>) {
        futures::executor::block_on(future);
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}

fn unsupported(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("MockRuntime does not implement {what}; it covers timers and tasks only"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;

    #[test]
    fn sleep_only_completes_when_time_advances() {
        let rt = MockRuntime::new();
        let clock = rt.clock();
        let mut sleep = rt.sleep(Duration::from_secs(10));

        assert!(sleep.as_mut().now_or_never().is_none(), "must not be ready");
        clock.advance(Duration::from_secs(9));
        assert!(
            sleep.as_mut().now_or_never().is_none(),
            "9s < 10s deadline, still pending"
        );
        clock.advance(Duration::from_secs(1));
        assert!(sleep.as_mut().now_or_never().is_some(), "deadline reached");
    }

    #[test]
    fn advance_is_instant_and_cumulative() {
        let rt = MockRuntime::new();
        let clock = rt.clock();
        assert_eq!(clock.elapsed(), Duration::ZERO);
        clock.advance(Duration::from_millis(250));
        clock.advance(Duration::from_millis(750));
        assert_eq!(clock.elapsed(), Duration::from_secs(1));
    }

    #[test]
    fn zero_duration_sleep_is_immediately_ready() {
        let rt = MockRuntime::new();
        assert!(rt.sleep(Duration::ZERO).now_or_never().is_some());
    }

    #[test]
    fn dropping_a_sleep_cancels_its_timer() {
        let rt = MockRuntime::new();
        let clock = rt.clock();
        let sleep = rt.sleep(Duration::from_secs(5));
        assert_eq!(clock.pending_timers(), 1);
        drop(sleep);
        assert_eq!(clock.pending_timers(), 0, "timer must not leak");
    }

    #[test]
    fn interval_first_tick_is_immediate_then_gated_by_clock() {
        let rt = MockRuntime::new();
        let clock = rt.clock();
        let mut interval = rt.interval(Duration::from_secs(1));

        assert!(
            interval.tick().now_or_never().is_some(),
            "first tick fires immediately"
        );
        let mut second = interval.tick();
        assert!(second.as_mut().now_or_never().is_none());
        clock.advance(Duration::from_secs(1));
        assert!(second.as_mut().now_or_never().is_some());
    }

    #[test]
    fn independent_runtimes_have_independent_clocks() {
        // This is what allows mock-based tests to run in parallel.
        let a = MockRuntime::new();
        let b = MockRuntime::new();
        a.clock().advance(Duration::from_secs(5));
        assert_eq!(a.clock().elapsed(), Duration::from_secs(5));
        assert_eq!(b.clock().elapsed(), Duration::ZERO);
    }

    #[test]
    fn timeout_helper_works_on_the_mock() {
        let rt = MockRuntime::new();
        let clock = rt.clock();
        // A future that never resolves must time out once the clock passes the deadline.
        let mut fut = Box::pin(super::super::timeout(
            &rt,
            Duration::from_secs(3),
            std::future::pending::<()>(),
        ));
        assert!(fut.as_mut().now_or_never().is_none());
        clock.advance(Duration::from_secs(3));
        assert!(matches!(fut.as_mut().now_or_never(), Some(Err(_))));
    }

    #[test]
    fn tcp_operations_report_unsupported() {
        let rt = MockRuntime::new();
        if let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:0") {
            let err = rt.wrap_tcp_listener(listener).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::Unsupported);
        }
        let err = futures::executor::block_on(
            rt.connect_tcp("127.0.0.1:1".parse().expect("literal addr")),
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    /// A datagram sent on the shared network lands in the peer's inbox with no wall-clock
    /// delay and no reactor: delivery happens inside `poll_send`.
    #[test]
    fn udp_datagrams_round_trip_in_memory() {
        let network = Arc::new(MockUDPNetwork::new());
        let a: SocketAddr = "127.0.0.1:4000".parse().expect("literal addr");
        let b: SocketAddr = "127.0.0.1:4001".parse().expect("literal addr");
        let sock_a = network.bind(a).expect("bind a");
        let sock_b = network.bind(b).expect("bind b");

        assert_eq!(
            network.bind(a).unwrap_err().kind(),
            io::ErrorKind::AddrInUse,
            "a bound address cannot be bound twice"
        );

        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        // Nothing sent yet: the receiver parks.
        let mut buf = [0u8; 64];
        let mut bufs = [IoSliceMut::new(&mut buf)];
        let mut meta = [RecvMeta::default()];
        assert!(sock_b.poll_recv(&mut cx, &mut bufs, &mut meta).is_pending());

        let payload = b"hello";
        let sent = sock_a.poll_send(
            &mut cx,
            &Transmit {
                destination: b,
                ecn: None,
                contents: payload,
                segment_size: None,
                src_ip: None,
            },
        );
        assert!(matches!(sent, Poll::Ready(Ok(n)) if n == payload.len()));

        let got = sock_b.poll_recv(&mut cx, &mut bufs, &mut meta);
        assert!(matches!(got, Poll::Ready(Ok(1))));
        assert_eq!(meta[0].addr, a, "the source address is preserved");
        assert_eq!(meta[0].len, payload.len());
        assert!(meta[0].stride >= 1, "stride must never be zero");
        assert_eq!(&buf[..payload.len()], payload);
    }

    /// Sending to an address nobody bound is a silent drop, as UDP is.
    #[test]
    fn udp_send_to_unbound_address_is_dropped() {
        let network = Arc::new(MockUDPNetwork::new());
        let a: SocketAddr = "127.0.0.1:4002".parse().expect("literal addr");
        let sock_a = network.bind(a).expect("bind a");
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let sent = sock_a.poll_send(
            &mut cx,
            &Transmit {
                destination: "127.0.0.1:9999".parse().expect("literal addr"),
                ecn: None,
                contents: b"into the void",
                segment_size: None,
                src_ip: None,
            },
        );
        assert!(matches!(sent, Poll::Ready(Ok(_))), "send still succeeds");
    }

    #[test]
    fn resolve_host_accepts_literal_addresses() {
        let rt = MockRuntime::new();
        let addrs = futures::executor::block_on(rt.resolve_host("127.0.0.1:3478")).unwrap();
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].port(), 3478);
    }
}
