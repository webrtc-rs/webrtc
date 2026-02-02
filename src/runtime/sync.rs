//! Runtime-agnostic synchronization primitives
//!
//! This module provides abstractions over tokio and smol sync primitives
//! to allow the webrtc crate to work with multiple async runtimes.

use std::future::Future;
use std::pin::Pin;

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

// Tokio implementations
#[cfg(feature = "runtime-tokio")]
mod tokio_impl {
    use super::*;
    use std::sync::Arc;

    /// Tokio-based mutex wrapper
    pub struct TokioMutex<T: ?Sized>(pub Arc<tokio::sync::Mutex<T>>);

    impl<T: ?Sized> Clone for TokioMutex<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> TokioMutex<T> {
        pub fn new(value: T) -> Self {
            Self(Arc::new(tokio::sync::Mutex::new(value)))
        }

        /// Lock the mutex asynchronously
        pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, T> {
            self.0.lock().await
        }
    }

    impl<T: ?Sized + Send> AsyncMutex<T> for TokioMutex<T> {
        type Guard<'a>
            = tokio::sync::MutexGuard<'a, T>
        where
            T: 'a;

        fn lock(&self) -> Pin<Box<dyn Future<Output = Self::Guard<'_>> + Send + '_>> {
            Box::pin(self.0.lock())
        }
    }

    /// Tokio-based notify wrapper
    pub struct TokioNotify(pub Arc<tokio::sync::Notify>);

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
            Self(Arc::new(tokio::sync::Notify::new()))
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
    pub struct TokioSender<T>(pub tokio::sync::mpsc::UnboundedSender<T>);

    impl<T> Clone for TokioSender<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T: Send> TokioSender<T> {
        /// Send a value asynchronously
        pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
            self.0.send(value).map_err(|e| SendError(e.0))
        }

        /// Try to send a value without blocking
        pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
            self.0
                .send(value)
                .map_err(|e| TrySendError::Disconnected(e.0))
        }
    }

    impl<T: Send> AsyncSender<T> for TokioSender<T> {
        fn send(
            &self,
            value: T,
        ) -> Pin<Box<dyn Future<Output = Result<(), SendError<T>>> + Send + '_>> {
            Box::pin(async move { self.0.send(value).map_err(|e| SendError(e.0)) })
        }

        fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
            self.0
                .send(value)
                .map_err(|e| TrySendError::Disconnected(e.0))
        }
    }

    /// Tokio-based channel receiver
    pub struct TokioReceiver<T>(pub tokio::sync::mpsc::UnboundedReceiver<T>);

    impl<T: Send> TokioReceiver<T> {
        /// Receive a value asynchronously
        pub async fn recv(&mut self) -> Option<T> {
            self.0.recv().await
        }

        /// Try to receive a value without blocking
        pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv().map_err(|e| match e {
                tokio::sync::mpsc::error::TryRecvError::Empty => TryRecvError::Empty,
                tokio::sync::mpsc::error::TryRecvError::Disconnected => TryRecvError::Disconnected,
            })
        }
    }

    impl<T: Send> AsyncReceiver<T> for TokioReceiver<T> {
        fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<T>> + Send + '_>> {
            Box::pin(self.0.recv())
        }

        fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv().map_err(|e| match e {
                tokio::sync::mpsc::error::TryRecvError::Empty => TryRecvError::Empty,
                tokio::sync::mpsc::error::TryRecvError::Disconnected => TryRecvError::Disconnected,
            })
        }
    }

    /// Create a new unbounded channel
    pub fn channel<T: Send>() -> (TokioSender<T>, TokioReceiver<T>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (TokioSender(tx), TokioReceiver(rx))
    }
}

#[cfg(feature = "runtime-tokio")]
pub use tokio_impl::*;

// Smol implementations
#[cfg(feature = "runtime-smol")]
mod smol_impl {
    use super::*;
    use std::sync::Arc;

    /// Smol-based mutex wrapper
    pub struct SmolMutex<T: ?Sized>(pub Arc<smol::lock::Mutex<T>>);

    impl<T: ?Sized> Clone for SmolMutex<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> SmolMutex<T> {
        pub fn new(value: T) -> Self {
            Self(Arc::new(smol::lock::Mutex::new(value)))
        }

        /// Lock the mutex asynchronously
        pub async fn lock(&self) -> smol::lock::MutexGuard<'_, T> {
            self.0.lock().await
        }
    }

    impl<T: ?Sized + Send> AsyncMutex<T> for SmolMutex<T> {
        type Guard<'a>
            = smol::lock::MutexGuard<'a, T>
        where
            T: 'a;

        fn lock(&self) -> Pin<Box<dyn Future<Output = Self::Guard<'_>> + Send + '_>> {
            Box::pin(self.0.lock())
        }
    }

    /// Smol-based notify wrapper using Event
    pub struct SmolNotify(pub Arc<smol::lock::Mutex<(bool, Vec<smol::channel::Sender<()>>)>>);

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
            Self(Arc::new(smol::lock::Mutex::new((false, Vec::new()))))
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
            let (tx, rx) = smol::channel::bounded(1);
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
                let (tx, rx) = smol::channel::bounded(1);
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
    pub struct SmolSender<T>(pub smol::channel::Sender<T>);

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
                smol::channel::TrySendError::Full(v) => TrySendError::Full(v),
                smol::channel::TrySendError::Closed(v) => TrySendError::Disconnected(v),
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
                smol::channel::TrySendError::Full(v) => TrySendError::Full(v),
                smol::channel::TrySendError::Closed(v) => TrySendError::Disconnected(v),
            })
        }
    }

    /// Smol-based channel receiver
    pub struct SmolReceiver<T>(pub smol::channel::Receiver<T>);

    impl<T: Send> SmolReceiver<T> {
        /// Receive a value asynchronously
        pub async fn recv(&mut self) -> Option<T> {
            self.0.recv().await.ok()
        }

        /// Try to receive a value without blocking
        pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv().map_err(|e| match e {
                smol::channel::TryRecvError::Empty => TryRecvError::Empty,
                smol::channel::TryRecvError::Closed => TryRecvError::Disconnected,
            })
        }
    }

    impl<T: Send> AsyncReceiver<T> for SmolReceiver<T> {
        fn recv(&mut self) -> Pin<Box<dyn Future<Output = Option<T>> + Send + '_>> {
            Box::pin(async move { self.0.recv().await.ok() })
        }

        fn try_recv(&mut self) -> Result<T, TryRecvError> {
            self.0.try_recv().map_err(|e| match e {
                smol::channel::TryRecvError::Empty => TryRecvError::Empty,
                smol::channel::TryRecvError::Closed => TryRecvError::Disconnected,
            })
        }
    }

    /// Create a new unbounded channel
    pub fn channel<T: Send>() -> (SmolSender<T>, SmolReceiver<T>) {
        let (tx, rx) = smol::channel::unbounded();
        (SmolSender(tx), SmolReceiver(rx))
    }
}

#[cfg(feature = "runtime-smol")]
pub use smol_impl::*;

// Convenient type aliases for runtime-agnostic code
#[cfg(feature = "runtime-tokio")]
pub type Mutex<T> = TokioMutex<T>;
#[cfg(feature = "runtime-tokio")]
pub type Notify = TokioNotify;
#[cfg(feature = "runtime-tokio")]
pub type Sender<T> = TokioSender<T>;
#[cfg(feature = "runtime-tokio")]
pub type Receiver<T> = TokioReceiver<T>;

#[cfg(feature = "runtime-smol")]
pub type Mutex<T> = SmolMutex<T>;
#[cfg(feature = "runtime-smol")]
pub type Notify = SmolNotify;
#[cfg(feature = "runtime-smol")]
pub type Sender<T> = SmolSender<T>;
#[cfg(feature = "runtime-smol")]
pub type Receiver<T> = SmolReceiver<T>;
