//! Async Runtime Abstraction
//!
//! This module provides the [`Runtime`] trait, which abstracts all asynchronous operations
//! and primitives required by the WebRTC stack. This makes the `webrtc` crate runtime-agnostic,
//! allowing it to support multiple async runtimes through feature flags.
//!
//! # Active Runtime
//!
//! The active runtime is selected at compile time via Cargo features:
//! *   **`runtime-tokio` (default)**: Uses the Tokio runtime.
//! *   **`runtime-smol`**: Uses the smol runtime.
//!
//! This module exports concrete type aliases (e.g., [`Mutex`], [`Sender`], [`Receiver`], [`Interval`])
//! which map to the selected runtime's primitives, ensuring zero-cost abstraction without
//! dynamic dispatch in the hot path.

#![allow(clippy::type_complexity)]

use std::{fmt::Debug, future::Future, io, net::SocketAddr, pin::Pin, sync::Arc, time::Duration};

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

impl Drop for JoinHandle {
    fn drop(&mut self) {
        self.inner.detach();
    }
}

trait JoinHandleInner: Send + Sync {
    /// Detach the task so it keeps running independently after the handle is dropped.
    fn detach(&self);
    /// Cancel the task cooperatively.
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
    /// Returns a handle that can be used to abort or inspect the task.
    /// Dropping the handle detaches the task; the task keeps running until it
    /// completes or the runtime is shut down. Call `.abort()` to cancel explicitly.
    #[track_caller]
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> JoinHandle;

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
}

/// Abstract implementation of a UDP socket for runtime independence
///
/// Simple async wrapper around UDP sockets
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
    /// The channel is full.
    Full(T),
    /// The channel is disconnected.
    Disconnected(T),
}

/// Error returned when try_recv fails
#[derive(Debug)]
pub enum TryRecvError {
    /// The channel is empty.
    Empty,
    /// The channel is disconnected.
    Disconnected,
}

impl<T> std::fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "channel disconnected")
    }
}

impl<T: std::fmt::Debug> std::error::Error for SendError<T> {}

/// Error returned when a broadcast send fails (no receivers)
#[derive(Debug)]
pub struct BroadcastSendError<T>(pub T);

/// Error returned when a broadcast receive fails
#[derive(Debug)]
pub enum BroadcastRecvError {
    /// Channel closed, no more senders
    Closed,
    /// Receiver lagged behind; this many messages were skipped
    Lagged(u64),
}

impl<T> std::fmt::Display for BroadcastSendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "broadcast send failed: no receivers")
    }
}

impl<T: std::fmt::Debug> std::error::Error for BroadcastSendError<T> {}

impl std::fmt::Display for BroadcastRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BroadcastRecvError::Closed => write!(f, "broadcast channel closed"),
            BroadcastRecvError::Lagged(n) => write!(f, "broadcast receiver lagged by {n}"),
        }
    }
}

impl std::error::Error for BroadcastRecvError {}

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
        Some(std::sync::Arc::new(smol::SmolRuntime))
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
        Some(std::sync::Arc::new(smol::SmolRuntime))
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
pub use tokio::{
    TokioInterval, block_on, broadcast_channel, channel, interval, resolve_host, sleep, timeout,
    yield_now,
};
/// The concrete Interval type for the active runtime.
#[cfg(feature = "runtime-tokio")]
pub type Interval = TokioInterval;
/// The concrete Mutex type for the active runtime.
#[cfg(feature = "runtime-tokio")]
pub type Mutex<T> = tokio::TokioMutex<T>;
/// The concrete Notify type for the active runtime.
#[cfg(feature = "runtime-tokio")]
pub type Notify = tokio::TokioNotify;
/// The concrete channel Sender type for the active runtime.
#[cfg(feature = "runtime-tokio")]
pub type Sender<T> = tokio::TokioSender<T>;
/// The concrete channel Receiver type for the active runtime.
#[cfg(feature = "runtime-tokio")]
pub type Receiver<T> = tokio::TokioReceiver<T>;
/// The concrete broadcast channel Sender type for the active runtime.
#[cfg(feature = "runtime-tokio")]
pub type BroadcastSender<T> = tokio::TokioBroadcastSender<T>;
/// The concrete broadcast channel Receiver type for the active runtime.
#[cfg(feature = "runtime-tokio")]
pub type BroadcastReceiver<T> = tokio::TokioBroadcastReceiver<T>;

#[cfg(feature = "runtime-smol")]
mod smol;

#[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
pub use smol::SmolRuntime;
#[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
pub use smol::{
    SmolInterval, block_on, broadcast_channel, channel, interval, resolve_host, sleep, timeout,
    yield_now,
};
/// The concrete Interval type for the active runtime.
#[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
pub type Interval = SmolInterval;
/// The concrete Mutex type for the active runtime.
#[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
pub type Mutex<T> = smol::SmolMutex<T>;
/// The concrete Notify type for the active runtime.
#[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
pub type Notify = smol::SmolNotify;
/// The concrete channel Sender type for the active runtime.
#[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
pub type Sender<T> = smol::SmolSender<T>;
/// The concrete channel Receiver type for the active runtime.
#[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
pub type Receiver<T> = smol::SmolReceiver<T>;
/// The concrete broadcast channel Sender type for the active runtime.
#[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
pub type BroadcastSender<T> = smol::SmolBroadcastSender<T>;
/// The concrete broadcast channel Receiver type for the active runtime.
#[cfg(all(not(feature = "runtime-tokio"), feature = "runtime-smol"))]
pub type BroadcastReceiver<T> = smol::SmolBroadcastReceiver<T>;
