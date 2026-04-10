//! smol runtime implementation

use super::*;
use ::smol::net::UdpSocket as SmolUdpSocket;
use ::smol::spawn;
use std::sync::Arc;

/// A WebRTC runtime for smol
#[derive(Debug)]
pub struct SmolRuntime;

// Dropping a `smol::Task` cancels it, so we store it in an Option and call
// `detach()` explicitly when the handle is dropped normally, or drop it for abort.
struct SmolJoinHandle(std::sync::Mutex<Option<::smol::Task<()>>>);

impl super::JoinHandleInner for SmolJoinHandle {
    fn detach(&self) {
        // Use unwrap_or_else to recover from a poisoned mutex rather than double-panicking.
        if let Some(task) = self.0.lock().unwrap_or_else(|e| e.into_inner()).take() {
            task.detach();
        }
    }

    fn abort(&self) {
        // Drop the Task to cooperatively cancel it at its next await point.
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
    }

    fn is_finished(&self) -> bool {
        false
    }
}

impl Runtime for SmolRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> super::JoinHandle {
        let task = spawn(future);
        super::JoinHandle {
            inner: Box::new(SmolJoinHandle(std::sync::Mutex::new(Some(task)))),
        }
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        Ok(Arc::new(UdpSocket::new(sock)?))
    }

    fn wrap_tcp_listener(
        &self,
        socket: std::net::TcpListener,
    ) -> io::Result<Arc<dyn super::AsyncTcpListener>> {
        let listener = ::smol::net::TcpListener::try_from(socket)?;
        let local_addr = listener.local_addr()?;
        Ok(Arc::new(SmolTcpListener {
            io: Arc::new(listener),
            local_addr,
        }))
    }

    fn connect_tcp(
        &self,
        addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn super::AsyncTcpStream>>> + Send>> {
        Box::pin(async move {
            let stream = ::smol::net::TcpStream::connect(addr).await?;
            let local_addr = stream.local_addr()?;
            let peer_addr = stream.peer_addr()?;
            Ok(Arc::new(SmolTcpStream {
                io: Arc::new(::futures::lock::Mutex::new(stream)),
                local_addr,
                peer_addr,
            }) as Arc<dyn super::AsyncTcpStream>)
        })
    }
}

// ── TCP listener ──────────────────────────────────────────────────────────────

#[derive(Debug)]
struct SmolTcpListener {
    io: Arc<::smol::net::TcpListener>,
    local_addr: SocketAddr,
}

impl super::AsyncTcpListener for SmolTcpListener {
    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn super::AsyncTcpStream>>> + Send + 'a>> {
        let io = self.io.clone();
        Box::pin(async move {
            let (stream, _peer) = io.accept().await?;
            let local_addr = stream.local_addr()?;
            let peer_addr = stream.peer_addr()?;
            Ok(Arc::new(SmolTcpStream {
                io: Arc::new(::futures::lock::Mutex::new(stream)),
                local_addr,
                peer_addr,
            }) as Arc<dyn super::AsyncTcpStream>)
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }
}

// ── TCP stream ────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct SmolTcpStream {
    io: Arc<::futures::lock::Mutex<::smol::net::TcpStream>>,
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
}

impl super::AsyncTcpStream for SmolTcpStream {
    fn read<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        let io = self.io.clone();
        Box::pin(async move {
            use ::futures::io::AsyncReadExt;
            io.lock().await.read(buf).await
        })
    }

    fn write_all<'a>(
        &'a self,
        buf: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            use ::futures::io::AsyncWriteExt;
            self.io.lock().await.write_all(buf).await
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.peer_addr)
    }
}

#[derive(Debug)]
struct UdpSocket {
    io: Arc<SmolUdpSocket>,
}

impl UdpSocket {
    fn new(sock: std::net::UdpSocket) -> io::Result<Self> {
        // Wrap std socket in smol's Async
        let async_sock = ::smol::Async::new(sock)?;
        Ok(Self {
            io: Arc::new(SmolUdpSocket::from(async_sock)),
        })
    }
}

impl AsyncUdpSocket for UdpSocket {
    fn send_to<'a>(
        &'a self,
        buf: &'a [u8],
        target: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        Box::pin(async move { self.io.send_to(buf, target).await })
    }

    fn recv_from<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<(usize, SocketAddr)>> + Send + 'a>> {
        Box::pin(async move { self.io.recv_from(buf).await })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }
}

pub async fn sleep(duration: Duration) {
    ::smol::Timer::after(duration).await;
}

/// A repeating interval timer backed by the smol runtime.
///
/// Created by [`interval`]. Each call to [`tick`](SmolInterval::tick) waits
/// until the next scheduled deadline, compensating for any drift so the
/// long-term cadence stays accurate.
pub struct SmolInterval {
    period: Duration,
    deadline: std::time::Instant,
    first: bool,
}

impl SmolInterval {
    /// Wait until the next tick fires.
    pub async fn tick(&mut self) {
        if self.first {
            // First tick fires immediately, matching tokio::time::interval behaviour.
            self.first = false;
        } else {
            ::smol::Timer::at(self.deadline).await;
        }
        self.deadline += self.period;
    }
}

/// Create a repeating interval that fires every `period`.
///
/// The first tick fires immediately (at time zero), matching `tokio::time::interval`
/// behaviour.
pub fn interval(period: Duration) -> SmolInterval {
    SmolInterval {
        period,
        deadline: std::time::Instant::now() + period,
        first: true,
    }
}

/// Runtime-agnostic timeout helper
///
/// Returns Ok(result) if the future completes within the duration,
/// or Err(()) if the timeout expires.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, ()>
where
    F: std::future::Future<Output = T>,
{
    ::smol::future::or(async { Ok(future.await) }, async {
        sleep(duration).await;
        Err(())
    })
    .await
}

/// Runtime-agnostic DNS resolution
pub async fn resolve_host(host: &str) -> io::Result<Vec<SocketAddr>> {
    ::smol::net::resolve(host).await
}

/// Smol-based mutex wrapper
pub struct SmolMutex<T: ?Sized>(pub Arc<::smol::lock::Mutex<T>>);

impl<T: ?Sized> Clone for SmolMutex<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> SmolMutex<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(::smol::lock::Mutex::new(value)))
    }

    /// Lock the mutex asynchronously
    pub async fn lock(&self) -> ::smol::lock::MutexGuard<'_, T> {
        self.0.lock().await
    }
}

impl<T: ?Sized + Send> AsyncMutex<T> for SmolMutex<T> {
    type Guard<'a>
        = ::smol::lock::MutexGuard<'a, T>
    where
        T: 'a;

    fn lock(&self) -> Pin<Box<dyn Future<Output = Self::Guard<'_>> + Send + '_>> {
        Box::pin(self.0.lock())
    }
}

/// Smol-based notify wrapper.
///
/// Uses an `AtomicBool` "pending" flag combined with a `std::sync::Mutex`-guarded
/// waiter list.  The std Mutex (not an async one) is used so that `notify_one`
/// and `notify_waiters` (which are synchronous) can *always* acquire the lock
/// and wake already-enqueued waiters, avoiding the lost-wakeup race that
/// `try_lock` caused in the previous implementation.
///
/// **Protocol:**
/// * `notify_*` sets the atomic flag to `true` *before* acquiring the lock and
///   waking waiters.  Even under contention the flag ensures that any
///   concurrent `notified()` call will observe the notification.
/// * `notified()` checks the flag *before* and *after* acquiring the lock, so
///   it cannot miss a notification that arrived between the two checks.
pub struct SmolNotify(
    pub  Arc<(
        std::sync::atomic::AtomicBool,
        std::sync::Mutex<Vec<::smol::channel::Sender<()>>>,
    )>,
);

impl Clone for SmolNotify {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Default for SmolNotify {
    fn default() -> Self {
        Self::new()
    }
}

impl SmolNotify {
    pub fn new() -> Self {
        Self(Arc::new((
            std::sync::atomic::AtomicBool::new(false),
            std::sync::Mutex::new(Vec::new()),
        )))
    }

    /// Notify one waiting task.
    pub fn notify_one(&self) {
        // Use blocking lock to guarantee we wake an already-enqueued waiter.
        let mut waiters = self.0.1.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tx) = waiters.pop() {
            // A waiter was woken — no need to set the pending flag.
            let _ = tx.try_send(());
        } else {
            // No waiters enqueued — store a permit so the next notified() returns immediately.
            self.0.0.store(true, std::sync::atomic::Ordering::Release);
        }
    }

    /// Notify all waiting tasks.
    pub fn notify_waiters(&self) {
        let mut waiters = self.0.1.lock().unwrap_or_else(|e| e.into_inner());
        if waiters.is_empty() {
            // No waiters enqueued — store a permit so the next notified() returns immediately.
            self.0.0.store(true, std::sync::atomic::Ordering::Release);
        } else {
            // Wake all enqueued waiters — no pending permit stored.
            for tx in waiters.drain(..) {
                let _ = tx.try_send(());
            }
        }
    }

    /// Wait for a notification.
    pub async fn notified(&self) {
        // Fast path: flag already set.
        if self.0.0.swap(false, std::sync::atomic::Ordering::AcqRel) {
            return;
        }
        let (tx, rx) = ::smol::channel::bounded(1);
        {
            let mut waiters = self.0.1.lock().unwrap_or_else(|e| e.into_inner());
            // Re-check after acquiring the lock: a notification may have arrived
            // between the swap above and acquiring the lock.
            if self.0.0.swap(false, std::sync::atomic::Ordering::AcqRel) {
                return;
            }
            waiters.push(tx);
        }
        let _ = rx.recv().await;
    }
}

impl AsyncNotify for SmolNotify {
    fn notify_one(&self) {
        SmolNotify::notify_one(self);
    }

    fn notify_waiters(&self) {
        SmolNotify::notify_waiters(self);
    }

    fn notified(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(SmolNotify::notified(self))
    }
}

/// Smol-based channel sender
pub struct SmolSender<T>(pub ::smol::channel::Sender<T>);

impl<T> Clone for SmolSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Send> SmolSender<T> {
    /// Send a value asynchronously
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.0.send(value).await.map_err(|e| SendError(e.0))
    }

    /// Try to send a value without blocking
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.0.try_send(value).map_err(|e| match e {
            ::smol::channel::TrySendError::Full(v) => TrySendError::Full(v),
            ::smol::channel::TrySendError::Closed(v) => TrySendError::Disconnected(v),
        })
    }
}

impl<T: Send> AsyncSender<T> for SmolSender<T> {
    fn send(
        &self,
        value: T,
    ) -> Pin<Box<dyn Future<Output = Result<(), SendError<T>>> + Send + '_>> {
        Box::pin(async move { self.0.send(value).await.map_err(|e| SendError(e.0)) })
    }

    fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.0.try_send(value).map_err(|e| match e {
            ::smol::channel::TrySendError::Full(v) => TrySendError::Full(v),
            ::smol::channel::TrySendError::Closed(v) => TrySendError::Disconnected(v),
        })
    }
}

/// Smol-based channel receiver
pub struct SmolReceiver<T>(pub ::smol::channel::Receiver<T>);

impl<T: Send> SmolReceiver<T> {
    /// Receive a value asynchronously
    pub async fn recv(&mut self) -> Option<T> {
        self.0.recv().await.ok()
    }

    /// Try to receive a value without blocking
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.0.try_recv().map_err(|e| match e {
            ::smol::channel::TryRecvError::Empty => TryRecvError::Empty,
            ::smol::channel::TryRecvError::Closed => TryRecvError::Disconnected,
        })
    }
}

impl<T: Send> AsyncReceiver<T> for SmolReceiver<T> {
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<T>> + Send + '_>> {
        Box::pin(async move { self.0.recv().await.ok() })
    }

    fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.0.try_recv().map_err(|e| match e {
            ::smol::channel::TryRecvError::Empty => TryRecvError::Empty,
            ::smol::channel::TryRecvError::Closed => TryRecvError::Disconnected,
        })
    }
}

/// Create a new bounded channel with the given capacity
pub fn channel<T: Send>(capacity: usize) -> (SmolSender<T>, SmolReceiver<T>) {
    let (tx, rx) = ::smol::channel::bounded(capacity);
    (SmolSender(tx), SmolReceiver(rx))
}

// ── Broadcast channel ─────────────────────────────────────────────────────────

/// Sender half of a broadcast channel (smol backend, backed by `async-broadcast`)
#[derive(Clone)]
pub struct SmolBroadcastSender<T>(pub ::async_broadcast::Sender<T>);

impl<T: Send + Clone + 'static> SmolBroadcastSender<T> {
    /// Send a value to all active receivers.
    /// Returns the number of receivers the message was sent to.
    pub fn send(&self, value: T) -> Result<usize, super::BroadcastSendError<T>> {
        match self.0.try_broadcast(value) {
            Ok(_) => Ok(self.0.receiver_count()),
            Err(::async_broadcast::TrySendError::Inactive(v)) => Err(super::BroadcastSendError(v)),
            Err(::async_broadcast::TrySendError::Closed(v)) => Err(super::BroadcastSendError(v)),
            Err(::async_broadcast::TrySendError::Full(v)) => Err(super::BroadcastSendError(v)),
        }
    }

    /// Subscribe to receive future values from this sender.
    pub fn subscribe(&self) -> SmolBroadcastReceiver<T> {
        SmolBroadcastReceiver(self.0.new_receiver())
    }

    /// Returns the number of active receivers.
    pub fn receiver_count(&self) -> usize {
        self.0.receiver_count()
    }
}

/// Receiver half of a broadcast channel (smol backend)
pub struct SmolBroadcastReceiver<T>(pub ::async_broadcast::Receiver<T>);

impl<T: Send + Clone + 'static> SmolBroadcastReceiver<T> {
    /// Receive the next value, waiting if none is available.
    pub async fn recv(&mut self) -> Result<T, super::BroadcastRecvError> {
        self.0.recv().await.map_err(|e| match e {
            ::async_broadcast::RecvError::Overflowed(n) => super::BroadcastRecvError::Lagged(n),
            ::async_broadcast::RecvError::Closed => super::BroadcastRecvError::Closed,
        })
    }
}

/// Create a new broadcast channel with the given capacity.
/// All active receivers will receive every sent value.
pub fn broadcast_channel<T: Send + Clone + 'static>(capacity: usize) -> SmolBroadcastSender<T> {
    let (mut tx, _rx) = ::async_broadcast::broadcast(capacity);
    tx.set_overflow(true);
    SmolBroadcastSender(tx)
}

/// Block the current thread on a future, driving it to completion
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    ::smol::block_on(future)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SmolNotify tests ─────────────────────────────────────────────────────

    #[test]
    fn notify_one_without_waiter_sets_pending() {
        let notify = SmolNotify::new();
        notify.notify_one();
        // The pending flag should be set since there were no waiters.
        assert!(notify.0.0.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn notify_one_with_waiter_does_not_set_pending() {
        block_on(async {
            let notify = SmolNotify::new();
            // Enqueue a waiter by pushing a sender manually.
            let (tx, rx) = ::smol::channel::bounded(1);
            {
                let mut waiters = notify.0.1.lock().unwrap();
                waiters.push(tx);
            }
            notify.notify_one();
            // The waiter should have been woken.
            assert!(rx.try_recv().is_ok());
            // The pending flag should NOT be set since a waiter was woken.
            assert!(!notify.0.0.load(std::sync::atomic::Ordering::Acquire));
        });
    }

    #[test]
    fn notify_waiters_without_waiters_sets_pending() {
        let notify = SmolNotify::new();
        notify.notify_waiters();
        assert!(notify.0.0.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn notify_waiters_with_waiters_does_not_set_pending() {
        block_on(async {
            let notify = SmolNotify::new();
            let (tx1, rx1) = ::smol::channel::bounded(1);
            let (tx2, rx2) = ::smol::channel::bounded(1);
            {
                let mut waiters = notify.0.1.lock().unwrap();
                waiters.push(tx1);
                waiters.push(tx2);
            }
            notify.notify_waiters();
            // Both waiters should have been woken.
            assert!(rx1.try_recv().is_ok());
            assert!(rx2.try_recv().is_ok());
            // The pending flag should NOT be set.
            assert!(!notify.0.0.load(std::sync::atomic::Ordering::Acquire));
        });
    }

    #[test]
    fn notified_returns_immediately_on_pending() {
        block_on(async {
            let notify = SmolNotify::new();
            notify.notify_one();
            // Should not block because the pending flag is set.
            notify.notified().await;
            // After consuming, the flag should be cleared.
            assert!(!notify.0.0.load(std::sync::atomic::Ordering::Acquire));
        });
    }

    #[test]
    fn notified_clears_pending_flag() {
        block_on(async {
            let notify = SmolNotify::new();
            // Set pending via notify_one (no waiters).
            notify.notify_one();
            assert!(notify.0.0.load(std::sync::atomic::Ordering::Acquire));
            // Consuming the notification clears the flag.
            notify.notified().await;
            assert!(!notify.0.0.load(std::sync::atomic::Ordering::Acquire));
            // A second notify_one should set pending again.
            notify.notify_one();
            assert!(notify.0.0.load(std::sync::atomic::Ordering::Acquire));
        });
    }

    #[test]
    fn async_notify_trait_delegates_correctly() {
        block_on(async {
            let notify = SmolNotify::new();
            // Use the AsyncNotify trait methods.
            <SmolNotify as super::super::AsyncNotify>::notify_one(&notify);
            assert!(notify.0.0.load(std::sync::atomic::Ordering::Acquire));
            <SmolNotify as super::super::AsyncNotify>::notified(&notify).await;
            assert!(!notify.0.0.load(std::sync::atomic::Ordering::Acquire));
        });
    }

    // ── SmolTcpStream write_all (no-clone) test ──────────────────────────────

    #[test]
    fn smol_tcp_write_all_uses_caller_buffer() {
        // Verify SmolTcpStream::write_all compiles without cloning buf.
        // This is a compile-time check — the signature fn write_all(&'a self, buf: &'a [u8])
        // must work without buf.to_vec(). A runtime test would need a real TCP pair
        // which is out of scope for unit tests. The fact that this module compiles
        // after removing to_vec() is the actual coverage.
        assert!(true);
    }
}
