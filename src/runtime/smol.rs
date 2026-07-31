//! smol runtime implementation

use super::*;
use ::smol::spawn;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};

/// A WebRTC runtime for smol
#[derive(Debug)]
pub struct SmolRuntime;

// Dropping a `smol::Task` cancels it, so we store it in an Option and call
// `detach()` explicitly when the handle is dropped normally, or drop it for abort.
struct SmolJoinHandle(std::sync::Mutex<Option<::smol::Task<()>>>);

/// `smol::Task` cancels on drop, but `JoinHandle` requires drop to *detach*.
impl Drop for SmolJoinHandle {
    fn drop(&mut self) {
        self.detach();
    }
}

impl super::JoinHandle for SmolJoinHandle {
    fn detach(&self) {
        if let Some(task) = self.0.lock().unwrap().take() {
            task.detach();
        }
    }

    fn abort(&self) {
        // Drop the Task to cooperatively cancel it at its next await point.
        self.0.lock().unwrap().take();
    }

    fn is_finished(&self) -> bool {
        // Once detached the task is untracked here (treated as finished); otherwise
        // report the underlying task's completion. `PeerConnection`'s drop path polls
        // this to wait, bounded, for the driver task to actually stop.
        self.0
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|task| task.is_finished())
    }
}

/// Shared, bounded pool of single-threaded reactor executors, replacing the old
/// one-OS-thread-per-`PeerConnection` model. Each slot is a dedicated thread that
/// runs exactly one `smol::Executor` via `block_on(ex.run(pending()))`, created
/// lazily on first use and kept alive for the process lifetime. Because a given
/// executor is run by only that one thread, its tasks never migrate off it, so
/// the thread (and per-thread allocator arena) count stays bounded by the pool
/// size regardless of connection count (issue #101 RSS). See
/// [`Runtime::spawn_reactor`].
struct ReactorPool {
    /// One lazily-initialised pool slot. `Some(executor)` is a live pool thread's
    /// executor; `None` records that this slot's thread could not be spawned (a rare
    /// resource-exhaustion failure), so its work falls back to the global executor
    /// rather than panicking.
    slots: Box<[OnceLock<Option<Arc<::smol::Executor<'static>>>>]>,
    next: AtomicUsize,
}

impl ReactorPool {
    fn new(size: usize) -> Self {
        let size = size.clamp(1, super::MAX_REACTOR_POOL_SIZE);
        let slots = (0..size)
            .map(|_| OnceLock::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            slots,
            next: AtomicUsize::new(0),
        }
    }

    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> ::smol::Task<()> {
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.slots.len();
        match self.slots[idx].get_or_init(|| spawn_reactor_thread(idx)) {
            Some(executor) => executor.spawn(future),
            // Slot thread failed to start; degrade to the global executor rather
            // than losing the driver (it lacks thread-pinning, but keeps the
            // connection alive).
            None => spawn(future),
        }
    }
}

/// Spawn one pool thread — a dedicated OS thread that runs a single `Executor`
/// forever — and return an `Arc` to that executor, or `None` if the thread could
/// not be spawned (a rare resource-exhaustion failure; the caller degrades to the
/// global executor). Because only this thread ever calls `run` on the executor,
/// tasks spawned onto it stay pinned to this thread (no work-stealing / migration).
/// smol's I/O reactor is process-global, so sockets wrapped inside those tasks are
/// pollable here.
fn spawn_reactor_thread(idx: usize) -> Option<Arc<::smol::Executor<'static>>> {
    let executor = Arc::new(::smol::Executor::new());
    let thread_executor = executor.clone();
    let spawned = std::thread::Builder::new()
        // Keep <= 15 bytes so the name survives Linux's `comm` truncation.
        .name(format!("webrtc-rx{idx}"))
        .spawn(move || {
            ::smol::block_on(thread_executor.run(std::future::pending::<()>()));
        });
    match spawned {
        Ok(_) => Some(executor),
        Err(err) => {
            log::error!("failed to spawn reactor pool thread: {err}");
            None
        }
    }
}

/// Process-global reactor pool for the smol runtime, sized once on first use.
static REACTOR_POOL: OnceLock<ReactorPool> = OnceLock::new();

impl Runtime for SmolRuntime {
    fn spawn(
        &self,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn super::JoinHandle> {
        let task = spawn(future);
        Box::new(SmolJoinHandle(std::sync::Mutex::new(Some(task))))
    }

    fn spawn_reactor(
        &self,
        reactor_pool_size: usize,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn super::JoinHandle> {
        // Route to the process-global bounded reactor pool (built lazily, sized
        // once from `reactor_pool_size`). The driver runs as a task pinned to one
        // pool thread; the returned handle aborts that task, not a whole thread.
        let task = REACTOR_POOL
            .get_or_init(|| ReactorPool::new(reactor_pool_size))
            .spawn(future);
        Box::new(SmolJoinHandle(std::sync::Mutex::new(Some(task))))
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        Ok(Arc::new(UdpSocket::new(sock)?) as Arc<dyn AsyncUdpSocket>)
    }

    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>> {
        listener.set_nonblocking(true)?;
        Ok(Arc::new(TcpListener::new(listener)?))
    }

    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>> {
        Box::pin(async move {
            let std_stream = std::net::TcpStream::connect(remote_addr)?;
            std_stream.set_nonblocking(true)?;
            let std_stream2 = std_stream.try_clone()?;
            let read_io = ::smol::Async::new(std_stream)?;
            let write_io = ::smol::Async::new(std_stream2)?;
            let local_addr = read_io.get_ref().local_addr()?;
            let peer_addr = read_io.get_ref().peer_addr()?;
            Ok(Arc::new(TcpStream {
                read_io,
                write_io,
                local_addr,
                peer_addr,
            }) as Arc<dyn AsyncTcpStream>)
        })
    }

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>> {
        Box::pin(async move { ::smol::net::resolve(host).await })
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            ::smol::Timer::after(duration).await;
        })
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        Box::new(SmolInterval {
            period,
            deadline: std::time::Instant::now() + period,
            first: true,
        })
    }

    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>) {
        ::smol::block_on(future);
    }

    fn name(&self) -> &'static str {
        "smol"
    }
}

/// A repeating interval timer backed by smol.
///
/// Waits until the next scheduled deadline, compensating for drift so the long-term
/// cadence stays accurate. The first tick fires immediately, matching
/// `tokio::time::interval`.
struct SmolInterval {
    period: Duration,
    deadline: std::time::Instant,
    first: bool,
}

impl AsyncInterval for SmolInterval {
    fn tick(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            if self.first {
                self.first = false;
            } else {
                ::smol::Timer::at(self.deadline).await;
            }
            self.deadline += self.period;
        })
    }
}

struct UdpSocket {
    io: Arc<::smol::Async<std::net::UdpSocket>>,
    /// Per-socket GSO/GRO capability state (see [`super::primitives`]).
    batch: Arc<UdpSocketState>,
}

/// Hand-written because `quinn_udp::UdpSocketState` implements no `Debug`, so `UdpSocket`
/// cannot derive it — and `AsyncUdpSocket` requires it.
impl std::fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UdpSocket")
            .field("io", &self.io)
            .field("max_gso_segments", &self.batch.max_gso_segments())
            .field("gro_segments", &self.batch.gro_segments())
            .finish()
    }
}

impl UdpSocket {
    fn new(sock: std::net::UdpSocket) -> io::Result<Self> {
        // Wrap std socket in smol's Async (sets non-blocking).
        let async_sock = ::smol::Async::new(sock)?;
        let batch = UdpSocketState::new(::quinn_udp::UdpSockRef::from(async_sock.get_ref()))?;
        Ok(Self {
            io: Arc::new(async_sock),
            batch: Arc::new(batch),
        })
    }
}

impl AsyncUdpSocket for UdpSocket {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    fn max_gso_segments(&self) -> usize {
        self.batch.max_gso_segments()
    }

    fn max_gro_segments(&self) -> usize {
        self.batch.gro_segments()
    }

    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        loop {
            // Attempt the syscall BEFORE consulting readiness, mirroring `Async::write_with`.
            // async-io's readiness is event/ticket-based: `poll_writable` reports a *newly
            // delivered* event and consumes it. Polling first and acting second would strand
            // capacity that is already available — no fresh event is emitted for it — so a
            // burst would stall waiting on an event that never comes.
            match self
                .batch
                .try_send(::quinn_udp::UdpSockRef::from(self.io.get_ref()), transmit)
                .map(|()| transmit.contents.len())
            {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                other => return Poll::Ready(other),
            }
            match self.io.poll_writable(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                // Event delivered: retry the syscall.
                Poll::Ready(Ok(())) => continue,
            }
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        loop {
            // Syscall first, for the same reason as `poll_send`: datagrams already queued in
            // the socket buffer produce no new readability event, so reading only after a
            // fresh event would leave them stranded and stall a burst.
            match self.batch.recv(
                ::quinn_udp::UdpSockRef::from(self.io.get_ref()),
                &mut *bufs,
                &mut *meta,
            ) {
                // Nothing queued: fall through and wait for a readability event.
                Ok(0) => {}
                Ok(n) => {
                    // `quinn-udp` mirrors `len` into `stride`, so a zero-length datagram
                    // reports `stride == 0` and would make de-segmentation divide by zero.
                    for m in &mut meta[..n] {
                        if m.stride == 0 {
                            m.stride = m.len.max(1);
                        }
                    }
                    return Poll::Ready(Ok(n));
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Poll::Ready(Err(e)),
            }
            match self.io.poll_readable(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => continue,
            }
        }
    }
}

#[derive(Debug)]
struct TcpListener {
    io: ::smol::Async<std::net::TcpListener>,
}

impl TcpListener {
    fn new(listener: std::net::TcpListener) -> io::Result<Self> {
        let async_listener = ::smol::Async::new(listener)?;
        Ok(Self { io: async_listener })
    }
}

impl AsyncTcpListener for TcpListener {
    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = io::Result<(Arc<dyn AsyncTcpStream>, SocketAddr)>> + Send + 'a>>
    {
        Box::pin(async move {
            let (std_stream, addr) = self.io.read_with(|io| io.accept()).await?;
            std_stream.set_nonblocking(true)?;
            let std_stream2 = std_stream.try_clone()?;
            let read_io = ::smol::Async::new(std_stream)?;
            let write_io = ::smol::Async::new(std_stream2)?;
            let local_addr = read_io.get_ref().local_addr()?;
            let peer_addr = read_io.get_ref().peer_addr()?;
            Ok((
                Arc::new(TcpStream {
                    read_io,
                    write_io,
                    local_addr,
                    peer_addr,
                }) as Arc<dyn AsyncTcpStream>,
                addr,
            ))
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }
}

#[derive(Debug)]
struct TcpStream {
    read_io: ::smol::Async<std::net::TcpStream>,
    write_io: ::smol::Async<std::net::TcpStream>,
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
}

impl AsyncTcpStream for TcpStream {
    fn read<'a, 'b>(
        &'a self,
        buf: &'b mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'b>>
    where
        'a: 'b,
    {
        Box::pin(async move { self.read_io.read_with(|mut io| io.read(buf)).await })
    }

    fn write_all<'a, 'b>(
        &'a self,
        buf: &'b [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'b>>
    where
        'a: 'b,
    {
        Box::pin(async move { self.write_io.write_with(|mut io| io.write_all(buf)).await })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.peer_addr)
    }
}
