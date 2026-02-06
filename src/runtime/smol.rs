//! smol runtime implementation

use super::*;
use ::smol::net::UdpSocket as SmolUdpSocket;
use ::smol::spawn;
use std::sync::Arc;

/// A WebRTC runtime for smol
#[derive(Debug)]
pub struct SmolRuntime;

struct SmolJoinHandle(::smol::Task<()>);

impl super::JoinHandleInner for SmolJoinHandle {
    fn abort(&self) {
        // smol doesn't have built-in abort, but dropping detached tasks stops them
        // We could use cancel() if we had a way to cancel, but Task<()> doesn't expose this
        // For now, we'll just do nothing as smol tasks are cooperative
    }

    fn is_finished(&self) -> bool {
        // smol::Task doesn't have is_finished, so we assume it's not finished
        false
    }
}

impl Runtime for SmolRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> super::JoinHandle {
        let task = spawn(future);
        super::JoinHandle {
            inner: Box::new(SmolJoinHandle(task)),
        }
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        Ok(Arc::new(UdpSocket::new(sock)?))
    }

    fn now(&self) -> Instant {
        Instant::now()
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

pub async fn sleep(duration: Duration) {
    ::smol::Timer::after(duration).await;
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
pub struct SmolNotify(pub Arc<::smol::lock::Mutex<(bool, Vec<::smol::channel::Sender<()>>)>>);

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
        Self(Arc::new(::smol::lock::Mutex::new((false, Vec::new()))))
    }

    /// Notify one waiting task
    pub fn notify_one(&self) {
        // Simple broadcast-based notification
        if let Some(mut state) = self.0.try_lock() {
            state.0 = true;
            if let Some(tx) = state.1.pop() {
                let _ = tx.try_send(());
            }
        }
    }

    /// Notify all waiting tasks
    pub fn notify_waiters(&self) {
        if let Some(mut state) = self.0.try_lock() {
            state.0 = true;
            for tx in state.1.drain(..) {
                let _ = tx.try_send(());
            }
        }
    }

    /// Wait for a notification
    pub async fn notified(&self) {
        let notify = self.0.clone();
        let (tx, rx) = ::smol::channel::bounded(1);
        {
            let mut state = notify.lock().await;
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
        // Simple broadcast-based notification
        if let Some(mut state) = self.0.try_lock() {
            state.0 = true;
            if let Some(tx) = state.1.pop() {
                let _ = tx.try_send(());
            }
        }
    }

    fn notify_waiters(&self) {
        if let Some(mut state) = self.0.try_lock() {
            state.0 = true;
            for tx in state.1.drain(..) {
                let _ = tx.try_send(());
            }
        }
    }

    fn notified(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let notify = self.0.clone();
        Box::pin(async move {
            let (tx, rx) = ::smol::channel::bounded(1);
            {
                let mut state = notify.lock().await;
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

/// Create a new unbounded channel
pub fn channel<T: Send>() -> (SmolSender<T>, SmolReceiver<T>) {
    let (tx, rx) = ::smol::channel::unbounded();
    (SmolSender(tx), SmolReceiver(rx))
}
