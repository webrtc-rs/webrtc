//! Tokio runtime implementation

use super::*;
use std::sync::Arc;

/// A WebRTC runtime for Tokio
#[derive(Debug)]
pub struct TokioRuntime;

struct TokioJoinHandle(::tokio::task::JoinHandle<()>);

impl super::JoinHandleInner for TokioJoinHandle {
    fn detach(&self) {
        // tokio JoinHandle detaches on drop; nothing needed here.
    }

    fn abort(&self) {
        self.0.abort();
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

impl Runtime for TokioRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> super::JoinHandle {
        let handle = ::tokio::spawn(future);
        super::JoinHandle {
            inner: Box::new(TokioJoinHandle(handle)),
        }
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        sock.set_nonblocking(true)?;
        Ok(Arc::new(UdpSocket {
            io: Arc::new(::tokio::net::UdpSocket::from_std(sock)?),
        }))
    }
}

#[derive(Debug, Clone)]
struct UdpSocket {
    io: Arc<::tokio::net::UdpSocket>,
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

/// Runtime-agnostic sleep function
pub async fn sleep(duration: Duration) {
    ::tokio::time::sleep(duration).await
}

/// A repeating interval timer backed by the Tokio runtime.
///
/// Created by [`interval`]. Each call to [`tick`](TokioInterval::tick) waits
/// until the next scheduled tick, maintaining consistent cadence even if
/// individual ticks are delayed.
pub struct TokioInterval(::tokio::time::Interval);

impl TokioInterval {
    /// Wait until the next tick fires.
    pub async fn tick(&mut self) {
        self.0.tick().await;
    }
}

/// Create a repeating interval that fires every `period`.
///
/// The first tick fires immediately (at time zero), matching `tokio::time::interval`
/// behaviour.
pub fn interval(period: Duration) -> TokioInterval {
    TokioInterval(::tokio::time::interval(period))
}

/// Runtime-agnostic timeout helper
///
/// Returns Ok(result) if the future completes within the duration,
/// or Err(()) if the timeout expires.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, ()>
where
    F: std::future::Future<Output = T>,
{
    ::tokio::time::timeout(duration, future)
        .await
        .map_err(|_| ())
}

/// Runtime-agnostic DNS resolution
pub async fn resolve_host(host: &str) -> io::Result<Vec<SocketAddr>> {
    ::tokio::net::lookup_host(host)
        .await
        .map(|iter| iter.collect())
}

/// Tokio-based mutex wrapper
pub struct TokioMutex<T: ?Sized>(pub Arc<::tokio::sync::Mutex<T>>);

impl<T: ?Sized> Clone for TokioMutex<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> TokioMutex<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(::tokio::sync::Mutex::new(value)))
    }

    /// Lock the mutex asynchronously
    pub async fn lock(&self) -> ::tokio::sync::MutexGuard<'_, T> {
        self.0.lock().await
    }
}

impl<T: ?Sized + Send> AsyncMutex<T> for TokioMutex<T> {
    type Guard<'a>
        = ::tokio::sync::MutexGuard<'a, T>
    where
        T: 'a;

    fn lock(&self) -> Pin<Box<dyn Future<Output = Self::Guard<'_>> + Send + '_>> {
        Box::pin(self.0.lock())
    }
}

/// Tokio-based notify wrapper
pub struct TokioNotify(pub Arc<::tokio::sync::Notify>);

impl Clone for TokioNotify {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Default for TokioNotify {
    fn default() -> Self {
        Self::new()
    }
}

impl TokioNotify {
    pub fn new() -> Self {
        Self(Arc::new(::tokio::sync::Notify::new()))
    }

    /// Notify one waiting task
    pub fn notify_one(&self) {
        self.0.notify_one();
    }

    /// Notify all waiting tasks
    pub fn notify_waiters(&self) {
        self.0.notify_waiters();
    }

    /// Wait for a notification
    pub async fn notified(&self) {
        self.0.notified().await
    }
}

impl AsyncNotify for TokioNotify {
    fn notify_one(&self) {
        self.0.notify_one();
    }

    fn notify_waiters(&self) {
        self.0.notify_waiters();
    }

    fn notified(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.0.notified())
    }
}

/// Tokio-based channel sender
pub struct TokioSender<T>(pub ::tokio::sync::mpsc::Sender<T>);

impl<T> Clone for TokioSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Send> TokioSender<T> {
    /// Send a value asynchronously
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.0.send(value).await.map_err(|e| SendError(e.0))
    }

    /// Try to send a value without blocking
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.0.try_send(value).map_err(|e| match e {
            ::tokio::sync::mpsc::error::TrySendError::Full(v) => TrySendError::Full(v),
            ::tokio::sync::mpsc::error::TrySendError::Closed(v) => TrySendError::Disconnected(v),
        })
    }
}

impl<T: Send> AsyncSender<T> for TokioSender<T> {
    fn send(
        &self,
        value: T,
    ) -> Pin<Box<dyn Future<Output = Result<(), SendError<T>>> + Send + '_>> {
        Box::pin(async move { self.0.send(value).await.map_err(|e| SendError(e.0)) })
    }

    fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.0.try_send(value).map_err(|e| match e {
            ::tokio::sync::mpsc::error::TrySendError::Full(v) => TrySendError::Full(v),
            ::tokio::sync::mpsc::error::TrySendError::Closed(v) => TrySendError::Disconnected(v),
        })
    }
}

/// Tokio-based channel receiver
pub struct TokioReceiver<T>(pub ::tokio::sync::mpsc::Receiver<T>);

impl<T: Send> TokioReceiver<T> {
    /// Receive a value asynchronously
    pub async fn recv(&mut self) -> Option<T> {
        self.0.recv().await
    }

    /// Try to receive a value without blocking
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.0.try_recv().map_err(|e| match e {
            ::tokio::sync::mpsc::error::TryRecvError::Empty => TryRecvError::Empty,
            ::tokio::sync::mpsc::error::TryRecvError::Disconnected => TryRecvError::Disconnected,
        })
    }
}

impl<T: Send> AsyncReceiver<T> for TokioReceiver<T> {
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<T>> + Send + '_>> {
        Box::pin(self.0.recv())
    }

    fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.0.try_recv().map_err(|e| match e {
            ::tokio::sync::mpsc::error::TryRecvError::Empty => TryRecvError::Empty,
            ::tokio::sync::mpsc::error::TryRecvError::Disconnected => TryRecvError::Disconnected,
        })
    }
}

/// Create a new bounded channel with the given capacity
pub fn channel<T: Send>(capacity: usize) -> (TokioSender<T>, TokioReceiver<T>) {
    let (tx, rx) = ::tokio::sync::mpsc::channel(capacity);
    (TokioSender(tx), TokioReceiver(rx))
}

/// Block the current thread on a future, driving it to completion
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    ::tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(future)
}
