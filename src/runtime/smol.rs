//! smol runtime implementation

use super::*;
use ::smol::net::UdpSocket as SmolUdpSocket;
use ::smol::spawn;
use std::io::{Read, Write};
use std::sync::Arc;

/// A WebRTC runtime for smol
#[derive(Debug)]
pub struct SmolRuntime;

// Dropping a `smol::Task` cancels it, so we store it in an Option and call
// `detach()` explicitly when the handle is dropped normally, or drop it for abort.
struct SmolJoinHandle(std::sync::Mutex<Option<::smol::Task<()>>>);

impl super::JoinHandleInner for SmolJoinHandle {
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

    fn spawn_reactor(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> super::JoinHandle {
        let join = std::thread::Builder::new()
            // Keep <= 15 bytes so the name survives Linux's `comm` truncation.
            .name("webrtc-reactor".into())
            .spawn(move || {
                // Dedicated thread driving this connection's event loop to keep it
                // off the shared global executor. smol's reactor is process-global,
                // so sockets wrapped inside `future` are safe to poll here.
                // TODO(#101): this confines the driver to one thread but does not
                // pin that thread to a CPU core; a follow-up can set core affinity
                // via the `core_affinity` crate for cache/NUMA locality.
                ::smol::block_on(future);
            })
            .expect("failed to spawn dedicated reactor thread");
        super::reactor_join_handle(join)
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        Ok(Arc::new(UdpSocket::new(sock)?))
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

/// Yields execution and sleeps for the specified duration using the smol timer.
pub async fn sleep(duration: Duration) {
    ::smol::Timer::after(duration).await;
}

/// Runtime-agnostic cooperative yield: reschedule the current task so other
/// ready tasks (e.g. the peer-connection driver) get a turn.
pub async fn yield_now() {
    ::smol::future::yield_now().await;
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

/// Smol-based notify wrapper using Event
pub struct SmolNotify(pub Arc<std::sync::Mutex<(bool, Vec<::smol::channel::Sender<()>>)>>);

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
        Self(Arc::new(std::sync::Mutex::new((false, Vec::new()))))
    }

    /// Notify one waiting task
    pub fn notify_one(&self) {
        let mut state = self.0.lock().unwrap();
        state.0 = true;
        if let Some(tx) = state.1.pop() {
            let _ = tx.try_send(());
        }
    }

    /// Notify all waiting tasks
    pub fn notify_waiters(&self) {
        let mut state = self.0.lock().unwrap();
        state.0 = true;
        for tx in state.1.drain(..) {
            let _ = tx.try_send(());
        }
    }

    /// Wait for a notification
    pub async fn notified(&self) {
        let (tx, rx) = ::smol::channel::bounded(1);
        {
            let mut state = self.0.lock().unwrap();
            if state.0 {
                state.0 = false;
                return;
            }
            state.1.push(tx);
        }
        let _ = rx.recv().await;
    }
}

impl AsyncNotify for SmolNotify {
    fn notify_one(&self) {
        self.notify_one();
    }

    fn notify_waiters(&self) {
        self.notify_waiters();
    }

    fn notified(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let notify = self.0.clone();
        Box::pin(async move {
            let (tx, rx) = ::smol::channel::bounded(1);
            {
                let mut state = notify.lock().unwrap();
                if state.0 {
                    state.0 = false;
                    return;
                }
                state.1.push(tx);
            }
            let _ = rx.recv().await;
        })
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
