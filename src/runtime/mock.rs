//! Deterministic runtime for tests.
//!
//! [`MockRuntime`] drives timers from a [`VirtualClock`] instead of wall-clock time, so a
//! test can advance time instantly and deterministically:
//!
//! ```
//! # use std::sync::Arc;
//! # use std::time::Duration;
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
//! This runtime covers **timers and task execution**. Socket operations return
//! [`io::ErrorKind::Unsupported`]: in-memory transports are a planned follow-up. Use it for
//! timing- and protocol-logic tests, not for end-to-end connection tests.

use super::{AsyncInterval, AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, Runtime};
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

/// A manually advanced clock. Timers registered against it fire only when
/// [`advance`](Self::advance) moves past their deadline.
#[derive(Debug, Default)]
pub struct VirtualClock {
    state: Mutex<ClockState>,
}

#[derive(Debug, Default)]
struct ClockState {
    /// Time elapsed since the clock was created.
    now: Duration,
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
    pub fn now(&self) -> Duration {
        self.state.lock().expect("clock poisoned").now
    }

    /// Advance the clock by `delta`, waking every timer whose deadline has passed.
    ///
    /// Wakers are invoked after the internal lock is released, so a woken task may
    /// register a new timer without deadlocking.
    pub fn advance(&self, delta: Duration) {
        let wakers = {
            let mut state = self.state.lock().expect("clock poisoned");
            state.now += delta;
            let now = state.now;
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
        let deadline = state.now + delay;
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

/// A deterministic [`Runtime`] backed by a [`VirtualClock`].
///
/// See the [module docs](self) for usage and scope.
#[derive(Debug, Default)]
pub struct MockRuntime {
    clock: Arc<VirtualClock>,
}

impl MockRuntime {
    /// Create a runtime with a fresh clock at time zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle to this runtime's clock, for advancing time in tests.
    pub fn clock(&self) -> Arc<VirtualClock> {
        Arc::clone(&self.clock)
    }
}

impl Runtime for MockRuntime {
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

    fn wrap_udp_socket(&self, _socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        Err(unsupported("wrap_udp_socket"))
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
        assert_eq!(clock.now(), Duration::ZERO);
        clock.advance(Duration::from_millis(250));
        clock.advance(Duration::from_millis(750));
        assert_eq!(clock.now(), Duration::from_secs(1));
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
        assert_eq!(a.clock().now(), Duration::from_secs(5));
        assert_eq!(b.clock().now(), Duration::ZERO);
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
    fn socket_operations_report_unsupported() {
        let rt = MockRuntime::new();
        let sock = std::net::UdpSocket::bind("127.0.0.1:0");
        if let Ok(sock) = sock {
            let err = rt.wrap_udp_socket(sock).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::Unsupported);
        }
    }

    #[test]
    fn resolve_host_accepts_literal_addresses() {
        let rt = MockRuntime::new();
        let addrs = futures::executor::block_on(rt.resolve_host("127.0.0.1:3478")).unwrap();
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].port(), 3478);
    }
}
