//! Tokio runtime implementation

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

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
        let size = size.clamp(1, MAX_REACTOR_POOL_SIZE);
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

/// Process-global reactor pool, sized once on first use from [`reactor_pool_size`].
static REACTOR_POOL: OnceLock<ReactorPool> = OnceLock::new();

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

    fn spawn_reactor(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> super::JoinHandle {
        // Route to the process-global bounded reactor pool (built lazily, sized
        // once from `reactor_pool_size`). The driver runs as a task pinned to one
        // pool thread; the returned handle aborts that task, not a whole thread.
        let handle = REACTOR_POOL
            .get_or_init(|| ReactorPool::new(super::reactor_pool_size()))
            .spawn(future);
        super::JoinHandle {
            inner: Box::new(TokioJoinHandle(handle)),
        }
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        sock.set_nonblocking(true)?;
        let io = ::tokio::net::UdpSocket::from_std(sock)?;
        // Probe + enable UDP GSO/GRO (and ECN/MTU options) on the socket. This is a
        // one-time reconfiguration; `send_to`/`recv_from` keep working, but the recv
        // path must now use `recv_gro` to decode GRO-coalesced buffers.
        let state = ::quinn_udp::UdpSocketState::new(::quinn_udp::UdpSockRef::from(&io))?;
        Ok(Arc::new(UdpSocket {
            io: Arc::new(io),
            state: Arc::new(state),
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
}

#[derive(Debug, Clone)]
struct UdpSocket {
    io: Arc<::tokio::net::UdpSocket>,
    /// GSO/GRO capability + syscall helper for this socket (see `quinn-udp`).
    state: Arc<::quinn_udp::UdpSocketState>,
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

    fn max_gso_segments(&self) -> usize {
        self.state.max_gso_segments()
    }

    fn max_gro_segments(&self) -> usize {
        self.state.gro_segments()
    }

    fn send_segments<'a>(
        &'a self,
        buf: &'a [u8],
        segment_size: usize,
        target: SocketAddr,
        ecn: Option<u8>,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        Box::pin(async move {
            let transmit = ::quinn_udp::Transmit {
                destination: target,
                ecn: ecn.and_then(::quinn_udp::EcnCodepoint::from_bits),
                contents: buf,
                segment_size: Some(segment_size),
                src_ip: None,
            };
            loop {
                match self.io.try_io(::tokio::io::Interest::WRITABLE, || {
                    self.state
                        .send(::quinn_udp::UdpSockRef::from(&self.io), &transmit)
                }) {
                    Ok(()) => return Ok(buf.len()),
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        self.io.writable().await?;
                    }
                    Err(e) => return Err(e),
                }
            }
        })
    }

    fn recv_gro<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<GroRecv>> + Send + 'a>> {
        Box::pin(async move {
            let mut meta = [::quinn_udp::RecvMeta::default()];
            loop {
                let mut bufs = [std::io::IoSliceMut::new(buf)];
                let res = self.io.try_io(::tokio::io::Interest::READABLE, || {
                    self.state.recv(
                        ::quinn_udp::UdpSockRef::from(&self.io),
                        &mut bufs,
                        &mut meta,
                    )
                });
                // `bufs` (which borrows `buf`) is no longer used past this point, so the
                // borrow ends here and the `.await` below can re-borrow on the next loop.
                match res {
                    Ok(_) => {
                        let m = &meta[0];
                        return Ok(GroRecv {
                            len: m.len,
                            stride: if m.stride == 0 {
                                m.len.max(1)
                            } else {
                                m.stride
                            },
                            peer_addr: m.addr,
                        });
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        self.io.readable().await?;
                    }
                    Err(e) => return Err(e),
                }
            }
        })
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

/// Runtime-agnostic sleep function
pub async fn sleep(duration: Duration) {
    ::tokio::time::sleep(duration).await
}

/// Runtime-agnostic cooperative yield: reschedule the current task so other
/// ready tasks (e.g. the peer-connection driver) get a turn.
pub async fn yield_now() {
    ::tokio::task::yield_now().await
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

// ── Broadcast channel ─────────────────────────────────────────────────────────

/// Sender half of a broadcast channel (tokio backend)
#[derive(Clone)]
pub struct TokioBroadcastSender<T>(pub ::tokio::sync::broadcast::Sender<T>);

impl<T: Send + Clone + 'static> TokioBroadcastSender<T> {
    /// Send a value to all active receivers.
    /// Returns the number of receivers the message was sent to.
    pub fn send(&self, value: T) -> Result<usize, super::BroadcastSendError<T>> {
        self.0
            .send(value)
            .map_err(|e| super::BroadcastSendError(e.0))
    }

    /// Subscribe to receive future values from this sender.
    pub fn subscribe(&self) -> TokioBroadcastReceiver<T> {
        TokioBroadcastReceiver(self.0.subscribe())
    }

    /// Returns the number of active receivers.
    pub fn receiver_count(&self) -> usize {
        self.0.receiver_count()
    }
}

/// Receiver half of a broadcast channel (tokio backend)
pub struct TokioBroadcastReceiver<T>(pub ::tokio::sync::broadcast::Receiver<T>);

impl<T: Send + Clone + 'static> TokioBroadcastReceiver<T> {
    /// Receive the next value, waiting if none is available.
    pub async fn recv(&mut self) -> Result<T, super::BroadcastRecvError> {
        self.0.recv().await.map_err(|e| match e {
            ::tokio::sync::broadcast::error::RecvError::Closed => super::BroadcastRecvError::Closed,
            ::tokio::sync::broadcast::error::RecvError::Lagged(n) => {
                super::BroadcastRecvError::Lagged(n)
            }
        })
    }
}

/// Create a new broadcast channel with the given capacity.
/// All active receivers will receive every sent value.
pub fn broadcast_channel<T: Send + Clone + 'static>(capacity: usize) -> TokioBroadcastSender<T> {
    let (tx, _) = ::tokio::sync::broadcast::channel(capacity);
    TokioBroadcastSender(tx)
}

/// Block the current thread on a future, driving it to completion
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    ::tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(future)
}
