//! Tokio runtime implementation

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};

/// A WebRTC runtime for Tokio
#[derive(Debug)]
pub struct TokioRuntime;

/// Shared, bounded pool of single-threaded reactor runtimes, replacing the old
/// one-OS-thread-per-`PeerConnection` model. Each slot is a dedicated thread
/// hosting a `new_current_thread` runtime, created lazily on first use and kept
/// alive for the process lifetime; driver futures are assigned to slots
/// round-robin and pinned there, so the thread (and per-thread allocator arena)
/// count is bounded by the pool size regardless of connection count (issue #101
/// RSS). See [`Runtime::spawn_reactor`].
struct ReactorPool {
    /// One lazily-initialised pool slot. A slot's thread is created only when that
    /// slot is first assigned work, so `M` connections use `min(M, N)` threads (an
    /// unshared thread each below the bound, then sharing). `Some(handle)` is a live
    /// pool runtime; `None` records that this slot's thread could not be created
    /// (a rare resource-exhaustion failure), so its work falls back to the ambient
    /// runtime instead — never a panic out of `build()`.
    slots: Box<[OnceLock<Option<::tokio::runtime::Handle>>]>,
    /// Round-robin cursor over `slots`.
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

    fn spawn(
        &self,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> ::tokio::task::JoinHandle<()> {
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.slots.len();
        match self.slots[idx].get_or_init(|| spawn_reactor_thread(idx)) {
            Some(handle) => handle.spawn(future),
            // Slot thread failed to start; degrade to the ambient runtime rather
            // than losing the driver (spawn_reactor is always called from within a
            // tokio runtime context — the application's — so this is valid).
            None => ::tokio::spawn(future),
        }
    }
}

/// Spawn one pool thread — a dedicated OS thread hosting a current-thread tokio
/// runtime — and return a `Handle` onto it, or `None` if the thread or its runtime
/// could not be built (a rare resource-exhaustion failure; the caller degrades to
/// the ambient runtime). The thread parks on `block_on(pending())` forever, which
/// keeps the runtime's I/O and timer drivers running so it can drive every future
/// later `Handle::spawn`ed onto it while the block_on future itself never completes.
fn spawn_reactor_thread(idx: usize) -> Option<::tokio::runtime::Handle> {
    // Rendezvous: hand the freshly-built runtime's Handle back to this caller.
    let (tx, rx) = std::sync::mpsc::sync_channel::<::tokio::runtime::Handle>(0);
    let spawned = std::thread::Builder::new()
        // Keep <= 15 bytes so the name survives Linux's `comm` truncation.
        .name(format!("webrtc-rx{idx}"))
        .spawn(move || {
            match ::tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => {
                    // Hand back a Handle, then keep the runtime (and its I/O + timer
                    // drivers) alive so `Handle::spawn`ed driver tasks make progress
                    // while we park here forever.
                    let _ = tx.send(rt.handle().clone());
                    rt.block_on(std::future::pending::<()>());
                }
                Err(err) => {
                    // Dropping `tx` unblocks the rendezvous `recv` with an error.
                    log::error!("failed to build reactor pool runtime: {err}");
                }
            }
        });
    match spawned {
        // `recv` errs only if the thread dropped `tx` without sending, i.e. the
        // runtime build failed above — surface that as `None`, not a panic.
        Ok(_) => rx.recv().ok(),
        Err(err) => {
            log::error!("failed to spawn reactor pool thread: {err}");
            None
        }
    }
}

/// Process-global reactor pool for the tokio runtime, sized once on first use.
static REACTOR_POOL: OnceLock<ReactorPool> = OnceLock::new();

struct TokioJoinHandle(::tokio::task::JoinHandle<()>);

// No `impl Drop` needed: `tokio::task::JoinHandle` already detaches when dropped, which is
// exactly the contract `super::JoinHandle` requires.
impl super::JoinHandle for TokioJoinHandle {
    fn detach(&self) {
        // Nothing to do — tokio detaches on drop, and holding the handle does not cancel.
    }

    fn abort(&self) {
        self.0.abort();
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

impl Runtime for TokioRuntime {
    fn spawn(
        &self,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn super::JoinHandle> {
        let handle = ::tokio::spawn(future);
        Box::new(TokioJoinHandle(handle))
    }

    fn spawn_reactor(
        &self,
        reactor_pool_size: usize,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn super::JoinHandle> {
        // Route to the process-global bounded reactor pool (built lazily, sized
        // once from `reactor_pool_size`). The driver runs as a task pinned to one
        // pool thread; the returned handle aborts that task, not a whole thread.
        let handle = REACTOR_POOL
            .get_or_init(|| ReactorPool::new(reactor_pool_size))
            .spawn(future);
        Box::new(TokioJoinHandle(handle))
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        sock.set_nonblocking(true)?;
        let io = ::tokio::net::UdpSocket::from_std(sock)?;
        // Probe + enable UDP GSO/GRO (and ECN/MTU options) on the socket. This is a
        // one-time reconfiguration of the socket itself: plain sends still work, but a
        // receive may now return several datagrams coalesced into one buffer, so the read
        // path has to de-segment by `stride`.
        let batch = ::quinn_udp::UdpSocketState::new(::quinn_udp::UdpSockRef::from(&io))?;
        Ok(Arc::new(UdpSocket {
            io: Arc::new(io),
            batch: Arc::new(batch),
        }))
    }

    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>> {
        listener.set_nonblocking(true)?;
        Ok(Arc::new(TcpListener {
            io: ::tokio::net::TcpListener::from_std(listener)?,
        }))
    }

    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>> {
        Box::pin(async move {
            let stream = ::tokio::net::TcpStream::connect(remote_addr).await?;
            let local_addr = stream.local_addr()?;
            let peer_addr = stream.peer_addr()?;
            let (read_half, write_half) = stream.into_split();
            Ok(Arc::new(TcpStream {
                read_half,
                write_half,
                local_addr,
                peer_addr,
            }) as Arc<dyn AsyncTcpStream>)
        })
    }

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>> {
        Box::pin(async move {
            ::tokio::net::lookup_host(host)
                .await
                .map(|iter| iter.collect())
        })
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(::tokio::time::sleep(duration))
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        Box::new(TokioInterval(::tokio::time::interval(period)))
    }

    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>) {
        ::tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime")
            .block_on(future);
    }

    fn yield_now(&self) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Use tokio's own yield so this participates in its cooperative scheduling
        // budget, rather than the generic wake-once default.
        Box::pin(::tokio::task::yield_now())
    }

    fn name(&self) -> &'static str {
        "tokio"
    }
}

/// A repeating interval timer backed by the Tokio runtime.
struct TokioInterval(::tokio::time::Interval);

impl AsyncInterval for TokioInterval {
    fn tick(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.0.tick().await;
        })
    }
}

#[derive(Clone)]
struct UdpSocket {
    io: Arc<::tokio::net::UdpSocket>,
    /// Per-socket GSO/GRO capability state (see [`super::primitives`]).
    batch: Arc<::quinn_udp::UdpSocketState>,
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

impl AsyncUdpSocket for UdpSocket {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }

    fn max_gso_segments(&self) -> usize {
        self.batch.max_gso_segments()
    }

    fn max_gro_segments(&self) -> usize {
        self.batch.gro_segments()
    }

    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        loop {
            match self.io.poll_send_ready(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {}
            }
            // The syscall MUST go through `try_io`. tokio's readiness is cached: it only
            // learns the socket went unready when an operation reports `WouldBlock` back
            // through `try_io`/`try_send`. Calling quinn-udp on the raw fd directly would
            // leave that cache stale, so `poll_send_ready` would keep returning `Ready`
            // and this loop would spin hot inside a single poll.
            match self.io.try_io(::tokio::io::Interest::WRITABLE, || {
                self.batch
                    .try_send(::quinn_udp::UdpSockRef::from(&self.io), transmit)
                    .map(|()| transmit.contents.len())
            }) {
                // `try_io` has cleared the cached readiness, so the next iteration's
                // `poll_send_ready` registers the waker and yields `Pending`.
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
        loop {
            match self.io.poll_recv_ready(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {}
            }
            // Via `try_io` for the same readiness-bookkeeping reason as `poll_send`.
            let bufs = &mut *bufs;
            let meta = &mut *meta;
            match self.io.try_io(::tokio::io::Interest::READABLE, || {
                let n = self
                    .batch
                    .recv(::quinn_udp::UdpSockRef::from(&self.io), bufs, meta)?;
                if n == 0 {
                    // Report unreadiness the way `try_io` expects, so it clears the cached
                    // readiness and the next `poll_recv_ready` parks the waker.
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                // `quinn-udp` mirrors `len` into `stride`, so a zero-length datagram
                // reports `stride == 0` and would make de-segmentation divide by zero.
                for m in &mut meta[..n] {
                    if m.stride == 0 {
                        m.stride = m.len.max(1);
                    }
                }
                Ok(n)
            }) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                other => return Poll::Ready(other),
            }
        }
    }
}

#[derive(Debug)]
struct TcpListener {
    io: ::tokio::net::TcpListener,
}

impl AsyncTcpListener for TcpListener {
    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = io::Result<(Arc<dyn AsyncTcpStream>, SocketAddr)>> + Send + 'a>>
    {
        Box::pin(async move {
            let (stream, addr) = self.io.accept().await?;
            let local_addr = stream.local_addr()?;
            let peer_addr = stream.peer_addr()?;
            let (read_half, write_half) = stream.into_split();
            Ok((
                Arc::new(TcpStream {
                    read_half,
                    write_half,
                    local_addr,
                    peer_addr,
                }) as Arc<dyn AsyncTcpStream>,
                addr,
            ))
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }
}

#[derive(Debug)]
struct TcpStream {
    read_half: ::tokio::net::tcp::OwnedReadHalf,
    write_half: ::tokio::net::tcp::OwnedWriteHalf,
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
        Box::pin(async move {
            loop {
                self.read_half.readable().await?;
                match self.read_half.try_read(buf) {
                    Ok(n) => return Ok(n),
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) => return Err(e),
                }
            }
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
            let mut remaining = buf;
            while !remaining.is_empty() {
                self.write_half.writable().await?;
                match self.write_half.try_write(remaining) {
                    Ok(0) => {
                        return Err(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "failed to write any bytes",
                        ));
                    }
                    Ok(n) => remaining = &remaining[n..],
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) => return Err(e),
                }
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
