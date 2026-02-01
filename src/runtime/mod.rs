//! Runtime abstraction for async I/O and timer operations
//!
//! This module provides traits that abstract over different async runtimes,
//! allowing the WebRTC implementation to work with Tokio, async-std, smol, and others.

use std::{
    fmt::{self, Debug},
    future::Future,
    io::{self, IoSliceMut},
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

/// Abstracts I/O and timer operations for runtime independence
///
/// This trait allows the WebRTC implementation to work with different async runtimes
/// without being tightly coupled to any specific runtime.
pub trait Runtime: Send + Sync + Debug + 'static {
    /// Construct a timer that will expire at the given instant
    fn new_timer(&self, deadline: Instant) -> Pin<Box<dyn AsyncTimer>>;

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

/// Abstract implementation of an async timer for runtime independence
pub trait AsyncTimer: Send + Debug + 'static {
    /// Update the timer to expire at a new instant
    fn reset(self: Pin<&mut Self>, deadline: Instant);

    /// Check whether the timer has expired, and register to be woken if not
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()>;
}

/// Metadata for a received UDP datagram
#[derive(Debug, Clone)]
pub struct RecvMeta {
    /// The source address of the datagram
    pub addr: SocketAddr,
    /// The number of bytes in the datagram
    pub len: usize,
    /// The destination address (local address that received the packet)
    pub dst_addr: Option<SocketAddr>,
}

/// A UDP datagram to be transmitted
#[derive(Debug, Clone)]
pub struct Transmit<'a> {
    /// The destination address
    pub destination: SocketAddr,
    /// The payload to send
    pub contents: &'a [u8],
    /// Optional source address override
    pub src_addr: Option<SocketAddr>,
}

/// Abstract implementation of a UDP socket for runtime independence
pub trait AsyncUdpSocket: Send + Sync + Debug + 'static {
    /// Create a [`UdpSender`] that can send datagrams
    ///
    /// This allows multiple tasks to send on the same socket concurrently
    /// by creating separate sender instances, each with their own waker.
    fn create_sender(&self) -> Pin<Box<dyn UdpSender>>;

    /// Receive UDP datagrams, or register to be woken if receiving may succeed in the future
    fn poll_recv(
        &mut self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>>;

    /// Look up the local IP address and port used by this socket
    fn local_addr(&self) -> io::Result<SocketAddr>;

    /// Maximum number of datagrams that might be received in a single call
    fn max_receive_segments(&self) -> usize {
        1
    }

    /// Whether datagrams might get fragmented into multiple parts
    fn may_fragment(&self) -> bool {
        true
    }
}

/// An object for asynchronously writing to an associated [`AsyncUdpSocket`]
///
/// Any number of [`UdpSender`]s may exist for a single [`AsyncUdpSocket`]. Each [`UdpSender`] is
/// responsible for notifying at most one task for send readiness.
pub trait UdpSender: Send + Sync + Debug + 'static {
    /// Send a UDP datagram, or register to be woken if sending may succeed in the future
    fn poll_send(
        self: Pin<&mut Self>,
        transmit: &Transmit<'_>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>>;

    /// Maximum number of datagrams that may be sent in a single call
    fn max_transmit_segments(&self) -> usize {
        1
    }
}

pin_project_lite::pin_project! {
    /// A helper for constructing [`UdpSender`]s from an underlying socket type
    pub(crate) struct UdpSenderHelper<Socket, MakeWritableFn, WritableFut> {
        socket: Socket,
        make_writable_fn: MakeWritableFn,
        #[pin]
        writable_fut: Option<WritableFut>,
    }
}

impl<Socket, MakeWritableFn, WritableFut> Debug
    for UdpSenderHelper<Socket, MakeWritableFn, WritableFut>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UdpSender")
    }
}

impl<Socket, MakeWritableFn, WritableFut> UdpSenderHelper<Socket, MakeWritableFn, WritableFut> {
    /// Create a new UDP sender helper
    #[cfg(any(feature = "runtime-tokio", feature = "runtime-smol"))]
    pub(crate) fn new(socket: Socket, make_writable_fn: MakeWritableFn) -> Self {
        Self {
            socket,
            make_writable_fn,
            writable_fut: None,
        }
    }
}

impl<Socket, MakeWritableFn, WritableFut> UdpSender
    for UdpSenderHelper<Socket, MakeWritableFn, WritableFut>
where
    Socket: UdpSenderHelperSocket,
    MakeWritableFn: Fn(&Socket) -> WritableFut + Send + Sync + 'static,
    WritableFut: Future<Output = io::Result<()>> + Send + Sync + 'static,
{
    fn poll_send(
        self: Pin<&mut Self>,
        transmit: &Transmit<'_>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        let mut this = self.project();
        loop {
            if this.writable_fut.is_none() {
                this.writable_fut
                    .set(Some((this.make_writable_fn)(this.socket)));
            }

            let result =
                std::task::ready!(this.writable_fut.as_mut().as_pin_mut().unwrap().poll(cx));

            // Clear the future so a new one is created on next call
            this.writable_fut.set(None);

            // If waiting for writability failed, propagate the error
            result?;

            match this.socket.try_send(transmit) {
                // Socket wasn't actually writable, retry
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                // Either success or a real error
                result => return Poll::Ready(result),
            }
        }
    }

    fn max_transmit_segments(&self) -> usize {
        self.socket.max_transmit_segments()
    }
}

/// Helper trait for socket types used with [`UdpSenderHelper`]
pub(crate) trait UdpSenderHelperSocket: Send + Sync + 'static {
    /// Try to send a datagram if the socket is write-ready
    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()>;

    /// Maximum number of segments that can be sent
    fn max_transmit_segments(&self) -> usize;
}

/// Automatically select an appropriate runtime from those enabled at compile time
///
/// If `runtime-tokio` is enabled and this function is called from within a Tokio runtime context,
/// then `TokioRuntime` is returned. Otherwise, if `runtime-smol` is enabled, `SmolRuntime` is returned.
#[cfg(any(feature = "runtime-tokio", feature = "runtime-smol"))]
pub fn default_runtime() -> Option<std::sync::Arc<dyn Runtime>> {
    #[cfg(feature = "runtime-tokio")]
    {
        if ::tokio::runtime::Handle::try_current().is_ok() {
            return Some(std::sync::Arc::new(TokioRuntime));
        }
    }

    #[cfg(feature = "runtime-smol")]
    {
        return Some(std::sync::Arc::new(SmolRuntime));
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
