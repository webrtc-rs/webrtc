//! Runtime abstraction for async I/O and timer operations
//!
//! This module provides traits that abstract over different async runtimes,
//! allowing the WebRTC implementation to work with Tokio, async-std, smol, and others.

#![allow(clippy::type_complexity)]

use std::{fmt::Debug, future::Future, io, net::SocketAddr, pin::Pin, time::Instant};

pub mod net;
pub mod sync;
pub mod time;

// Re-export commonly used items for convenience
pub use net::{resolve_host, UdpSocket};
pub use time::{sleep, timeout};

/// Abstracts I/O and timer operations for runtime independence
///
/// This trait allows the WebRTC implementation to work with different async runtimes
/// without being tightly coupled to any specific runtime.
pub trait Runtime: Send + Sync + Debug + 'static {
    /// Drive a future to completion in the background
    ///
    /// The future must complete to `()` and will be spawned as a background task.
    #[track_caller]
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>);

    /// Create an async UDP socket from a standard socket
    ///
    /// The socket should be bound and configured before being wrapped.
    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Box<dyn AsyncUdpSocket>>;

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

#[cfg(feature = "runtime-smol")]
mod smol;
#[cfg(feature = "runtime-smol")]
pub use smol::SmolRuntime;
