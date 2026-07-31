//! A third-party async runtime for `webrtc`, over `async-executor` + `async-io`.
//!
//! Implements [`webrtc::runtime::Runtime`] using neither Tokio nor smol, demonstrating that
//! the runtime abstraction is genuinely pluggable: supply this type to
//! `PeerConnectionBuilder::with_runtime` and the whole stack — timers, task spawning,
//! sockets, DNS — runs on it.
//!
//! Kept in its own module so it has a single definition shared by two consumers:
//!
//! * `custom-runtime.rs`, the runnable example, and
//! * `tests/custom_runtime_interop.rs`, which drives a real peer connection on it alongside
//!   a second connection on the built-in runtime.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::Future;
use std::io::IoSliceMut;
use std::task::{Context, Poll};
use webrtc::runtime::{
    AsyncInterval, AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, RecvMeta, Runtime,
    Transmit,
};

// ── The runtime ───────────────────────────────────────────────────────────────

/// A minimal runtime over `async-executor` (task scheduling) and `async-io` (reactor).
#[derive(Debug)]
pub struct MyRuntime {
    executor: Arc<async_executor::Executor<'static>>,
}

impl MyRuntime {
    pub fn new() -> Self {
        let executor = Arc::new(async_executor::Executor::new());

        // Drive the executor on a small pool of threads. `async-io` runs its own reactor
        // thread on demand, so timers and socket readiness work without further setup.
        for i in 0..2 {
            let ex = Arc::clone(&executor);
            std::thread::Builder::new()
                .name(format!("my-rt-{i}"))
                .spawn(move || {
                    futures_lite::future::block_on(ex.run(std::future::pending::<()>()));
                })
                .expect("failed to spawn executor thread");
        }

        Self { executor }
    }
}

/// Handle wrapping an `async-task` so it can be aborted or polled for completion.
struct MyJoinHandle {
    task: std::sync::Mutex<Option<async_executor::Task<()>>>,
}

/// `async_executor::Task` cancels on drop, but `JoinHandle` requires drop to *detach*.
/// Omitting this is the classic way to make a custom runtime silently kill its own drivers.
impl Drop for MyJoinHandle {
    fn drop(&mut self) {
        self.detach();
    }
}

impl JoinHandle for MyJoinHandle {
    fn detach(&self) {
        if let Some(task) = self.task.lock().unwrap().take() {
            task.detach();
        }
    }

    fn abort(&self) {
        // Dropping an `async-executor` Task cancels it at its next await point.
        self.task.lock().unwrap().take();
    }

    fn is_finished(&self) -> bool {
        self.task
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|t| t.is_finished())
    }
}

impl Runtime for MyRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Box<dyn JoinHandle> {
        let task = self.executor.spawn(future);
        Box::new(MyJoinHandle {
            task: std::sync::Mutex::new(Some(task)),
        })
    }

    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        socket.set_nonblocking(true)?;
        Ok(Arc::new(MyUdpSocket {
            io: async_io::Async::new(socket)?,
        }))
    }

    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>> {
        listener.set_nonblocking(true)?;
        Ok(Arc::new(MyTcpListener {
            io: async_io::Async::new(listener)?,
        }))
    }

    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>> {
        Box::pin(async move {
            let stream = async_io::Async::<std::net::TcpStream>::connect(remote_addr).await?;
            let local_addr = stream.get_ref().local_addr()?;
            let peer_addr = stream.get_ref().peer_addr()?;
            Ok(Arc::new(MyTcpStream {
                io: stream,
                local_addr,
                peer_addr,
            }) as Arc<dyn AsyncTcpStream>)
        })
    }

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>> {
        // `std`'s resolver blocks, so run it off the executor threads.
        let host = host.to_owned();
        Box::pin(async move {
            blocking_task(move || {
                use std::net::ToSocketAddrs;
                host.to_socket_addrs().map(|it| it.collect::<Vec<_>>())
            })
            .await
        })
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            async_io::Timer::after(duration).await;
        })
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        Box::new(MyInterval {
            period,
            first: true,
        })
    }

    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>) {
        futures_lite::future::block_on(future);
    }

    fn name(&self) -> &'static str {
        "my-runtime"
    }
}

/// Run a blocking closure on its own thread and await the result.
async fn blocking_task<T, F>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let _ = tx.send_blocking(f());
    });
    rx.recv().await.expect("blocking task panicked")
}

/// Repeating timer. The first tick fires immediately, matching the built-in runtimes.
struct MyInterval {
    period: Duration,
    first: bool,
}

impl AsyncInterval for MyInterval {
    fn tick(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        if self.first {
            self.first = false;
            return Box::pin(std::future::ready(()));
        }
        let period = self.period;
        Box::pin(async move {
            async_io::Timer::after(period).await;
        })
    }
}

// ── Sockets ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct MyUdpSocket {
    io: async_io::Async<std::net::UdpSocket>,
}

impl AsyncUdpSocket for MyUdpSocket {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        // No GSO here: `max_gso_segments` stays at its default of 1, so the caller never
        // hands us a multi-segment buffer, and ECN marking is not applied. A production
        // runtime would batch via `webrtc::runtime::UdpSocketState`, handing it the whole
        // `Transmit`.
        debug_assert!(
            transmit
                .segment_size
                .is_none_or(|seg| seg >= transmit.contents.len())
        );
        loop {
            match self.io.poll_writable(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {}
            }
            match self
                .io
                .get_ref()
                .send_to(transmit.contents, transmit.destination)
            {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                other => return Poll::Ready(other),
            }
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        // The minimal shape the trait allows: fill one buffer per call and return `Ok(1)`,
        // as a platform without `recvmmsg` would. A production runtime would hand `bufs`
        // and `meta` straight to `UdpSocketState::recv`, which fills up to `BATCH_SIZE` of
        // them in one syscall.
        loop {
            match self.io.poll_readable(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {}
            }
            match self.io.get_ref().recv_from(&mut bufs[0]) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => return Poll::Ready(Err(e)),
                // No GRO: exactly one datagram, so `stride == len`. `RecvMeta` is
                // `#[non_exhaustive]`, so assign fields rather than using a literal.
                Ok((len, peer_addr)) => {
                    meta[0] = RecvMeta::default();
                    meta[0].len = len;
                    meta[0].stride = len.max(1);
                    meta[0].addr = peer_addr;
                    return Poll::Ready(Ok(1));
                }
            }
        }
    }
}

#[derive(Debug)]
struct MyTcpListener {
    io: async_io::Async<std::net::TcpListener>,
}

impl AsyncTcpListener for MyTcpListener {
    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = io::Result<(Arc<dyn AsyncTcpStream>, SocketAddr)>> + Send + 'a>>
    {
        Box::pin(async move {
            let (stream, peer_addr) = self.io.accept().await?;
            let local_addr = stream.get_ref().local_addr()?;
            Ok((
                Arc::new(MyTcpStream {
                    io: stream,
                    local_addr,
                    peer_addr,
                }) as Arc<dyn AsyncTcpStream>,
                peer_addr,
            ))
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }
}

#[derive(Debug)]
struct MyTcpStream {
    io: async_io::Async<std::net::TcpStream>,
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
}

impl AsyncTcpStream for MyTcpStream {
    fn read<'a, 'b>(
        &'a self,
        buf: &'b mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'b>>
    where
        'a: 'b,
    {
        Box::pin(async move {
            use std::io::Read;
            self.io.read_with(|mut s| s.read(buf)).await
        })
    }

    fn write_all<'a, 'b>(
        &'a self,
        buf: &'b [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'b>>
    where
        'a: 'b,
    {
        Box::pin(async move {
            use std::io::Write;
            let mut written = 0;
            while written < buf.len() {
                let n = self.io.write_with(|mut s| s.write(&buf[written..])).await?;
                if n == 0 {
                    return Err(io::ErrorKind::WriteZero.into());
                }
                written += n;
            }
            Ok(())
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.peer_addr)
    }
}
