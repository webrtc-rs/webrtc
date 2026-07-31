//! Runtime-agnostic building blocks.
//!
//! Everything here works under **any** async runtime and is therefore always compiled —
//! never feature-gated. Two distinct groups live in this module, agnostic for different
//! reasons:
//!
//! 1. **Waker-driven primitives** — [`Mutex`], [`Notify`], [`channel`], [`broadcast_channel`].
//!    Plain data structures that never register with a reactor, so one implementation
//!    suffices everywhere. Keeping them out of the [`Runtime`](super::Runtime) trait is also
//!    what keeps that trait object-safe: a generic method such as `fn channel<T>(&self, …)`
//!    could not be called through `dyn Runtime`.
//!
//! 2. **UDP socket types** — [`Transmit`], [`RecvMeta`], [`EcnCodepoint`], [`UdpSockRef`],
//!    [`UdpSocketState`] and [`BATCH_SIZE`], re-exported from `quinn-udp`. The first two
//!    appear in
//!    [`AsyncUdpSocket`](super::AsyncUdpSocket)'s signatures, so every runtime must be able
//!    to name them; the rest are what a runtime needs to enable GSO/GRO batching. Nothing
//!    is wrapped — the built-in runtimes drive `UdpSocketState` directly and an out-of-tree
//!    one can do the same — which does tie part of this crate's public API to `quinn-udp`'s
//!    major version.
//!
//! Reactor-bound operations — task spawning, timers, socket readiness, DNS — are *not* here;
//! they are injected through the [`Runtime`](super::Runtime) trait.

use std::sync::Arc;

/// Metadata describing one UDP receive: `len`, `stride`, `addr`, and the ECN codepoint.
///
/// Re-exported from `quinn-udp`, since it appears in [`AsyncUdpSocket::poll_recv`]'s
/// signature and so must be nameable by out-of-tree runtimes. When the kernel coalesced
/// consecutive same-flow datagrams via UDP GRO, `buf[..len]` holds several datagrams of
/// `stride` bytes each (the last may be shorter); without GRO `stride == len` and there is
/// exactly one. Every datagram in a batch shares `addr`.
///
/// It is `#[non_exhaustive]`, so build one with [`RecvMeta::default`] and assign fields
/// rather than using a struct literal.
///
/// [`AsyncUdpSocket::poll_recv`]: super::AsyncUdpSocket::poll_recv
pub use quinn_udp::RecvMeta;

/// One outbound UDP send: `contents` to `destination`, optionally segmented and ECN-marked.
///
/// Re-exported from `quinn-udp`, since it appears in [`AsyncUdpSocket::poll_send`]'s
/// signature. `segment_size` of `None` sends `contents` as a single datagram; `Some(n)`
/// requests UDP GSO, splitting it into consecutive `n`-byte datagrams emitted in one
/// syscall — valid only when the socket reports `max_gso_segments() > 1`.
///
/// [`AsyncUdpSocket::poll_send`]: super::AsyncUdpSocket::poll_send
pub use quinn_udp::Transmit;

/// ECN codepoint carried on [`Transmit::ecn`]. Build one from the raw two bits with
/// [`EcnCodepoint::from_bits`].
///
/// Re-exported from `quinn-udp`, since constructing a [`Transmit`] requires naming it.
pub use quinn_udp::EcnCodepoint;

/// Borrowed handle to a socket, as required by the batching syscalls.
///
/// Re-exported from `quinn-udp`. Build one with `UdpSockRef::from(&sock)` for any socket
/// implementing the platform's raw-fd/socket trait.
pub use quinn_udp::UdpSockRef;

/// Maximum messages one [`AsyncUdpSocket::poll_recv`](super::AsyncUdpSocket::poll_recv)
/// call can return: 32 where the platform has `recvmmsg` or an equivalent, otherwise 1.
///
/// Re-exported from `quinn-udp`, for sizing the buffer and metadata arrays.
pub use quinn_udp::BATCH_SIZE;

/// Per-socket UDP GSO/GRO capability state, established once at wrap time.
///
/// Re-exported from `quinn-udp`: construct with `UdpSocketState::new(sock)` to probe and
/// enable GSO/GRO (plus ECN and MTU options), then drive it from a socket's `poll_send` and
/// `poll_recv`. Its syscalls are synchronous and non-blocking, so the only runtime-specific
/// part is readiness, which each socket implementation supplies around them.
///
/// `recv` takes the same `bufs`/`meta` slices [`AsyncUdpSocket::poll_recv`] does and returns
/// the same message count, so an implementation forwards them rather than adapting them.
/// Two things it must still handle:
///
/// * **A count of `0`** means nothing was ready — translate it into whatever the host
///   runtime's readiness protocol expects, not into a received message.
/// * **`stride` must be at least 1** for every filled message: `quinn-udp` mirrors `len`
///   into `stride`, which is `0` for a zero-length datagram and would make de-segmentation
///   divide by zero.
///
/// Note also that it implements no `Debug`, so a socket type holding one cannot derive
/// `Debug` and must write the impl by hand to satisfy
/// [`AsyncUdpSocket`](super::AsyncUdpSocket)'s supertrait.
///
/// [`AsyncUdpSocket::poll_recv`]: super::AsyncUdpSocket::poll_recv
pub use quinn_udp::UdpSocketState;

// ── Mutex ─────────────────────────────────────────────────────────────────────

/// An async mutex.
///
/// Cheaply clonable (shares one lock), mirroring the previous runtime-specific wrappers
/// which were internally `Arc`-based.
#[derive(Debug, Default)]
pub struct Mutex<T: ?Sized>(Arc<futures::lock::Mutex<T>>);

impl<T: ?Sized> Clone for Mutex<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Mutex<T> {
    /// Create a new mutex holding `value`.
    pub fn new(value: T) -> Self {
        Self(Arc::new(futures::lock::Mutex::new(value)))
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Lock the mutex, waiting if it is currently held.
    pub async fn lock(&self) -> futures::lock::MutexGuard<'_, T> {
        self.0.lock().await
    }

    /// Try to lock without waiting. Returns `None` if another task holds the lock.
    pub fn try_lock(&self) -> Option<futures::lock::MutexGuard<'_, T>> {
        self.0.try_lock()
    }
}

// ── Notify ────────────────────────────────────────────────────────────────────

/// A notification primitive for waking parked tasks.
///
/// # Semantics
///
/// [`notify_waiters`](Self::notify_waiters) wakes every task **currently** waiting in
/// [`notified`](Self::notified) and does not store a permit for a future waiter — the
/// same contract as `tokio::sync::Notify::notify_waiters`. Callers must therefore
/// re-check their condition after waking, which is the standard usage pattern.
#[derive(Debug, Clone, Default)]
pub struct Notify(Arc<event_listener::Event>);

impl Notify {
    /// Create a new, unnotified `Notify`.
    pub fn new() -> Self {
        Self(Arc::new(event_listener::Event::new()))
    }

    /// Wake one waiting task, if any. No permit is stored when there are no waiters.
    pub fn notify_one(&self) {
        self.0.notify(1);
    }

    /// Wake all currently waiting tasks. No permit is stored when there are no waiters.
    pub fn notify_waiters(&self) {
        self.0.notify(usize::MAX);
    }

    /// Wait for a notification.
    ///
    /// The returned future only observes notifications published after it is created, so
    /// callers should re-check their condition once it completes.
    pub async fn notified(&self) {
        self.0.listen().await;
    }
}

// ── Bounded MPMC channel ──────────────────────────────────────────────────────

/// Error returned when a send fails because the channel is closed.
#[derive(Debug)]
pub struct SendError<T>(pub T);

/// Error returned by [`Sender::try_send`].
#[derive(Debug)]
pub enum TrySendError<T> {
    /// The channel is at capacity.
    Full(T),
    /// The channel is closed.
    Disconnected(T),
}

/// Error returned by [`Receiver::try_recv`].
#[derive(Debug)]
pub enum TryRecvError {
    /// No message is currently available.
    Empty,
    /// The channel is closed and drained.
    Disconnected,
}

impl<T> std::fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "channel disconnected")
    }
}

impl<T: std::fmt::Debug> std::error::Error for SendError<T> {}

/// Sending half of a bounded channel. Clonable.
#[derive(Debug)]
pub struct Sender<T>(async_channel::Sender<T>);

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Sender<T> {
    /// Send a value, waiting while the channel is full.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.0.send(value).await.map_err(|e| SendError(e.0))
    }

    /// Try to send without waiting.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.0.try_send(value).map_err(|e| match e {
            async_channel::TrySendError::Full(v) => TrySendError::Full(v),
            async_channel::TrySendError::Closed(v) => TrySendError::Disconnected(v),
        })
    }

    /// Returns `true` once every receiver has been dropped.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

/// Receiving half of a bounded channel.
///
/// `&mut self` receivers are retained from the previous single-consumer API so existing
/// call sites are unchanged, even though the underlying channel supports multiple
/// consumers.
#[derive(Debug)]
pub struct Receiver<T>(async_channel::Receiver<T>);

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Receiver<T> {
    /// Receive the next value, waiting while the channel is empty. Returns `None` once
    /// the channel is closed and drained.
    pub async fn recv(&mut self) -> Option<T> {
        self.0.recv().await.ok()
    }

    /// Try to receive without waiting.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.0.try_recv().map_err(|e| match e {
            async_channel::TryRecvError::Empty => TryRecvError::Empty,
            async_channel::TryRecvError::Closed => TryRecvError::Disconnected,
        })
    }

    /// Returns `true` once every sender has been dropped.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

/// Create a bounded channel with room for `capacity` messages.
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    // `async_channel::bounded` panics on 0; the previous tokio/smol wrappers did too,
    // but clamp to 1 so a computed capacity of 0 degrades instead of aborting.
    let (tx, rx) = async_channel::bounded(capacity.max(1));
    (Sender(tx), Receiver(rx))
}

// ── Broadcast channel ─────────────────────────────────────────────────────────

/// Error returned when a broadcast send fails because there are no receivers.
#[derive(Debug)]
pub struct BroadcastSendError<T>(pub T);

/// Error returned when a broadcast receive fails.
#[derive(Debug)]
pub enum BroadcastRecvError {
    /// The channel is closed and no senders remain.
    Closed,
    /// The receiver fell behind and this many messages were skipped.
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

/// Sending half of a broadcast channel: every live receiver observes every message.
///
/// Holds a deactivated receiver internally. `async-broadcast` closes a channel once its
/// last receiver is dropped, and a closed channel cannot be reopened by
/// [`subscribe`](Self::subscribe) — so retaining an inactive receiver keeps the channel
/// usable across periods with no subscribers, without accumulating messages for one.
#[derive(Debug)]
pub struct BroadcastSender<T> {
    tx: async_broadcast::Sender<T>,
    inactive: async_broadcast::InactiveReceiver<T>,
}

impl<T> Clone for BroadcastSender<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            inactive: self.inactive.clone(),
        }
    }
}

impl<T: Clone> BroadcastSender<T> {
    /// Broadcast a value to all currently subscribed receivers, returning how many
    /// receivers it was delivered to.
    ///
    /// **Never blocks.** A broadcaster must not stall because nobody is listening, so this
    /// is synchronous: it either delivers, drops the oldest queued message (overflow is
    /// enabled), or returns [`BroadcastSendError`] when there are no active receivers or
    /// the channel is closed.
    pub fn send(&self, value: T) -> Result<usize, BroadcastSendError<T>> {
        match self.tx.try_broadcast(value) {
            Ok(_) => Ok(self.tx.receiver_count()),
            Err(async_broadcast::TrySendError::Inactive(v)) => Err(BroadcastSendError(v)),
            Err(async_broadcast::TrySendError::Closed(v)) => Err(BroadcastSendError(v)),
            // Overflow is enabled, so a full queue evicts the oldest entry rather than
            // reporting `Full`; kept for exhaustiveness.
            Err(async_broadcast::TrySendError::Full(v)) => Err(BroadcastSendError(v)),
        }
    }

    /// Subscribe a new receiver. It observes messages sent after this call.
    pub fn subscribe(&self) -> BroadcastReceiver<T> {
        BroadcastReceiver(self.inactive.activate_cloned())
    }

    /// Number of currently active receivers. The retained inactive receiver is not
    /// counted.
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

/// Receiving half of a broadcast channel.
#[derive(Debug)]
pub struct BroadcastReceiver<T>(async_broadcast::Receiver<T>);

impl<T: Clone> Clone for BroadcastReceiver<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Clone> BroadcastReceiver<T> {
    /// Receive the next broadcast value.
    pub async fn recv(&mut self) -> Result<T, BroadcastRecvError> {
        match self.0.recv().await {
            Ok(v) => Ok(v),
            Err(async_broadcast::RecvError::Overflowed(n)) => Err(BroadcastRecvError::Lagged(n)),
            Err(async_broadcast::RecvError::Closed) => Err(BroadcastRecvError::Closed),
        }
    }
}

/// Create a broadcast channel holding up to `capacity` in-flight messages.
///
/// Overflow is enabled: a slow receiver is skipped past (surfacing as
/// [`BroadcastRecvError::Lagged`]) rather than stalling the sender.
pub fn broadcast_channel<T: Clone>(capacity: usize) -> BroadcastSender<T> {
    let (mut tx, rx) = async_broadcast::broadcast(capacity.max(1));
    tx.set_overflow(true);
    BroadcastSender {
        tx,
        inactive: rx.deactivate(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutex_shares_state_across_clones() {
        let a = Mutex::new(0u32);
        let b = a.clone();
        futures::executor::block_on(async {
            *a.lock().await = 7;
            assert_eq!(*b.lock().await, 7, "clones must share one lock");
        });
    }

    #[test]
    fn mutex_try_lock_fails_while_held() {
        let m = Mutex::new(1u32);
        futures::executor::block_on(async {
            let _g = m.lock().await;
            assert!(m.try_lock().is_none());
        });
        assert!(m.try_lock().is_some(), "released after guard drop");
    }

    #[test]
    fn channel_roundtrip_and_try_variants() {
        let (tx, mut rx) = channel::<u8>(2);
        futures::executor::block_on(async {
            tx.send(1).await.unwrap();
            tx.try_send(2).unwrap();
            assert!(matches!(tx.try_send(3), Err(TrySendError::Full(3))));
            assert_eq!(rx.recv().await, Some(1));
            assert_eq!(rx.try_recv().unwrap(), 2);
            assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
        });
    }

    #[test]
    fn channel_capacity_zero_is_clamped_not_panic() {
        let (tx, mut rx) = channel::<u8>(0);
        futures::executor::block_on(async {
            tx.send(9).await.unwrap();
            assert_eq!(rx.recv().await, Some(9));
        });
    }

    #[test]
    fn recv_returns_none_after_senders_dropped() {
        let (tx, mut rx) = channel::<u8>(1);
        drop(tx);
        assert_eq!(futures::executor::block_on(rx.recv()), None);
    }

    #[test]
    fn sender_reports_closed_when_receiver_dropped() {
        let (tx, rx) = channel::<u8>(1);
        assert!(!tx.is_closed());
        drop(rx);
        assert!(tx.is_closed());
    }

    #[test]
    fn broadcast_reaches_every_subscriber() {
        let tx = broadcast_channel::<u8>(4);
        let mut a = tx.subscribe();
        let mut b = tx.subscribe();
        assert_eq!(tx.send(5).unwrap(), 2, "delivered to both subscribers");
        futures::executor::block_on(async {
            assert_eq!(a.recv().await.unwrap(), 5);
            assert_eq!(b.recv().await.unwrap(), 5);
        });
    }

    #[test]
    fn broadcast_send_never_blocks_without_subscribers() {
        // A broadcaster must not stall because nobody is listening. `send` is synchronous
        // and reports the condition instead of awaiting a subscriber (an SFU with zero
        // viewers would otherwise wedge its publisher).
        let tx = broadcast_channel::<u8>(4);
        assert!(
            tx.send(1).is_err(),
            "no active receivers => Err, not a hang"
        );
        assert_eq!(tx.receiver_count(), 0);
    }

    #[test]
    fn broadcast_survives_losing_all_subscribers() {
        // `async-broadcast` closes a channel once its last receiver drops, and `subscribe`
        // cannot reopen it. The retained inactive receiver prevents that, so the channel
        // still works after its subscribers come and go.
        let tx = broadcast_channel::<u8>(4);
        futures::executor::block_on(async {
            {
                let mut early = tx.subscribe();
                assert_eq!(tx.send(2).unwrap(), 1);
                assert_eq!(early.recv().await.unwrap(), 2);
            } // active receiver count back to zero

            let mut late = tx.subscribe();
            assert_eq!(
                tx.send(3).unwrap(),
                1,
                "channel must remain open across a gap in subscribers"
            );
            assert_eq!(late.recv().await.unwrap(), 3);
        });
    }

    #[test]
    fn notify_waiters_wakes_a_parked_task() {
        // `notify_waiters` stores no permit, so the listener must be registered first —
        // exactly the re-check pattern documented on `Notify`.
        let n = Notify::new();
        futures::executor::block_on(async {
            let listener = n.0.listen();
            n.notify_waiters();
            listener.await;
        });
    }

    #[test]
    fn notify_with_no_waiters_stores_no_permit() {
        let n = Notify::new();
        n.notify_waiters();
        // A listener created afterwards must not already be signalled.
        let listener = n.0.listen();
        assert!(
            futures::FutureExt::now_or_never(listener).is_none(),
            "notify_waiters must not store a permit for future waiters"
        );
    }
}
