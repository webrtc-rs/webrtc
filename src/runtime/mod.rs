//! Runtime abstraction for async I/O and timer operations
//!
//! This module provides traits that abstract over different async runtimes,
//! allowing the WebRTC implementation to work with Tokio, async-std, smol, and others.

#![allow(clippy::type_complexity)]

use std::{
    fmt::Debug,
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

/// Handle to a spawned task that can be used to manage its lifecycle
pub struct JoinHandle {
    inner: Box<dyn JoinHandleInner>,
}

impl JoinHandle {
    /// Abort the spawned task
    pub fn abort(&self) {
        self.inner.abort();
    }

    /// Check if the task is finished
    pub fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }
}

trait JoinHandleInner: Send + Sync {
    fn abort(&self);
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
    /// Returns a handle that can be used to manage the task lifecycle.
    ///
    /// # Safety Note
    /// The returned JoinHandle must be properly managed - if it's dropped or aborted,
    /// the spawned task will be cancelled. This allows spawning non-'static futures
    /// as long as the handle outlives them.
    #[track_caller]
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> JoinHandle;

    /// Create an async UDP socket from a standard socket
    ///
    /// The socket should be bound and configured before being wrapped.
    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>>;

    /*
    /// Create an async TCP socket from a standard socket
    ///
    /// The socket should be bound and configured before being wrapped.
    fn wrap_tcp_listener(
        &self,
        socket: std::net::TcpListener,
    ) -> io::Result<Box<dyn AsyncTcpListener>>;*/

    /// Get the current time
    ///
    /// Allows simulating the flow of time for testing.
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Abstract implementation of a UDP socket for runtime independence
///
/// Simple async wrapper around UDP sockets, compatible with tokio::net::UdpSocket API
pub trait AsyncUdpSocket: Send + Sync + Debug + 'static {
    /// Send data to the specified address
    fn send_to<'a>(
        &'a self,
        buf: &'a [u8],
        target: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>>;

    /// Receive a datagram from the socket
    fn recv_from<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<(usize, SocketAddr)>> + Send + 'a>>;

    /// Get the local address this socket is bound to
    fn local_addr(&self) -> io::Result<SocketAddr>;
}

/// An async mutex that works across different runtimes
pub trait AsyncMutex<T: ?Sized>: Send + Sync {
    /// The guard type returned by lock()
    type Guard<'a>: std::ops::Deref<Target = T> + std::ops::DerefMut + Send + 'a
    where
        Self: 'a,
        T: 'a;

    /// Lock the mutex asynchronously
    fn lock(&self) -> Pin<Box<dyn Future<Output = Self::Guard<'_>> + Send + '_>>;
}

/// An async notification primitive
pub trait AsyncNotify: Send + Sync {
    /// Notify one waiting task
    fn notify_one(&self);

    /// Notify all waiting tasks
    fn notify_waiters(&self);

    /// Wait for a notification
    fn notified(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// Sender half of an async channel
pub trait AsyncSender<T>: Send + Sync {
    /// Send a value, waiting if the channel is full
    fn send(&self, value: T)
    -> Pin<Box<dyn Future<Output = Result<(), SendError<T>>> + Send + '_>>;

    /// Try to send a value without blocking
    fn try_send(&self, value: T) -> Result<(), TrySendError<T>>;
}

/// Receiver half of an async channel
pub trait AsyncReceiver<T>: Send {
    /// Receive a value, waiting if the channel is empty
    fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<T>> + Send + '_>>;

    /// Try to receive a value without blocking
    fn try_recv(&mut self) -> Result<T, TryRecvError>;
}

/// Error returned when send fails
#[derive(Debug)]
pub struct SendError<T>(pub T);

/// Error returned when try_send fails
#[derive(Debug)]
pub enum TrySendError<T> {
    Full(T),
    Disconnected(T),
}

/// Error returned when try_recv fails
#[derive(Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

impl<T> std::fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "channel disconnected")
    }
}

impl<T: std::fmt::Debug> std::error::Error for SendError<T> {}

/// Get the default runtime for the current build configuration
///
/// Returns the runtime for whichever runtime feature is enabled.
/// If multiple runtimes are enabled, tokio takes precedence.
#[cfg(any(feature = "runtime-tokio", feature = "runtime-smol"))]
pub fn default_runtime() -> Option<std::sync::Arc<dyn Runtime>> {
    #[cfg(feature = "runtime-tokio")]
    {
        Some(std::sync::Arc::new(TokioRuntime))
    }

    #[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
    {
        Some(std::sync::Arc::new(SmolRuntime))
    }
}

#[cfg(not(any(feature = "runtime-tokio", feature = "runtime-smol")))]
pub fn default_runtime() -> Option<std::sync::Arc<dyn Runtime>> {
    None
}

/// Get smol runtime if enabled
#[cfg(any(feature = "runtime-tokio", feature = "runtime-smol"))]
pub fn smol_runtime() -> Option<std::sync::Arc<dyn Runtime>> {
    #[cfg(feature = "runtime-smol")]
    {
        Some(std::sync::Arc::new(SmolRuntime))
    }

    #[cfg(not(feature = "runtime-smol"))]
    None
}

// Runtime implementations
#[cfg(feature = "runtime-tokio")]
mod tokio;
#[cfg(feature = "runtime-tokio")]
pub use tokio::TokioRuntime;
#[cfg(feature = "runtime-tokio")]
pub use tokio::{channel, resolve_host, sleep, timeout};
#[cfg(feature = "runtime-tokio")]
pub type Mutex<T> = tokio::TokioMutex<T>;
#[cfg(feature = "runtime-tokio")]
pub type Notify = tokio::TokioNotify;
#[cfg(feature = "runtime-tokio")]
pub type Sender<T> = tokio::TokioSender<T>;
#[cfg(feature = "runtime-tokio")]
pub type Receiver<T> = tokio::TokioReceiver<T>;

#[cfg(feature = "runtime-smol")]
mod smol;
#[cfg(feature = "runtime-smol")]
pub use smol::SmolRuntime;
#[cfg(feature = "runtime-smol")]
pub use smol::{channel, resolve_host, sleep, timeout};
#[cfg(feature = "runtime-smol")]
pub type Mutex<T> = smol::SmolMutex<T>;
#[cfg(feature = "runtime-smol")]
pub type Notify = smol::SmolNotify;
#[cfg(feature = "runtime-smol")]
pub type Sender<T> = smol::SmolSender<T>;
#[cfg(feature = "runtime-smol")]
pub type Receiver<T> = smol::SmolReceiver<T>;
