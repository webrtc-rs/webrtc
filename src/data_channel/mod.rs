//! DataChannel API
//!
//! This module provides the [`DataChannel`] trait and the [`DataChannelEvent`] enum, which are used
//! to establish bidirectional, low-latency, peer-to-peer data channels.
//!
//! # Concepts
//!
//! *   **[`DataChannel`]**: Represents a WebRTC data channel. It is created via
//!     [`PeerConnection::create_data_channel`](crate::peer_connection::PeerConnection::create_data_channel) or received via the
//!     [`PeerConnectionEventHandler::on_data_channel`](crate::peer_connection::PeerConnectionEventHandler::on_data_channel) callback.
//! *   **Event Polling**: Unlike the callback-heavy design of older versions, events (like opening,
//!     closing, receiving messages, or errors) are fetched by calling the asynchronous
//!     [`DataChannel::poll`] method in a loop.
//!
//! # Examples
//!
//! ## Sending and Receiving Data
//!
//! ```no_run
//! # use webrtc::data_channel::{DataChannel, DataChannelEvent};
//! # use std::sync::Arc;
//! # async fn handle_data_channel(dc: Arc<dyn DataChannel>) -> webrtc::error::Result<()> {
//! // Poll for events in a loop
//! while let Some(event) = dc.poll().await {
//!     match event {
//!         DataChannelEvent::OnOpen => {
//!             println!("Data channel opened!");
//!             dc.send_text("Hello, peer!").await?;
//!         }
//!         DataChannelEvent::OnMessage(msg) => {
//!             if let Some(text) = String::from_utf8(msg.data.to_vec()).ok() {
//!                 println!("Received text message: {}", text);
//!             }
//!         }
//!         DataChannelEvent::OnClose => {
//!             println!("Data channel closed");
//!             break;
//!         }
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use crate::peer_connection::PeerConnectionRef;
use crate::runtime::{Mutex, Receiver};
use bytes::BytesMut;
use futures::FutureExt;
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::shared::error::{Error, Result};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub use rtc::data_channel::{
    RTCDataChannelId, RTCDataChannelInit, RTCDataChannelMessage, RTCDataChannelState,
};

/// Object-safe trait exposing all public DataChannel operations.
///
/// This allows `on_data_channel` in `PeerConnectionEventHandler` to receive a
/// `Arc<dyn DataChannel>` without the event handler trait itself needing to
/// be generic over the interceptor type `I`.
#[async_trait::async_trait]
pub trait DataChannel: Send + Sync + 'static {
    /// Returns the label of this data channel.
    async fn label(&self) -> Result<String>;
    /// Returns whether this data channel guarantees in-order delivery.
    async fn ordered(&self) -> Result<bool>;
    /// Returns the maximum packet lifetime in milliseconds, if configured.
    async fn max_packet_life_time(&self) -> Result<Option<u16>>;
    /// Returns the maximum number of retransmissions, if configured.
    async fn max_retransmits(&self) -> Result<Option<u16>>;
    /// Returns the subprotocol name configured for this data channel.
    async fn protocol(&self) -> Result<String>;
    /// Returns whether this data channel was negotiated by the application.
    async fn negotiated(&self) -> Result<bool>;
    /// Returns the unique identifier of this data channel.
    fn id(&self) -> RTCDataChannelId;
    /// Returns the current state of this data channel.
    async fn ready_state(&self) -> Result<RTCDataChannelState>;
    /// Returns the buffered amount high threshold in bytes.
    async fn buffered_amount_high_threshold(&self) -> Result<u32>;
    /// Sets the buffered amount high threshold in bytes.
    async fn set_buffered_amount_high_threshold(&self, threshold: u32) -> Result<()>;
    /// Returns the buffered amount low threshold in bytes.
    async fn buffered_amount_low_threshold(&self) -> Result<u32>;
    /// Sets the buffered amount low threshold in bytes.
    async fn set_buffered_amount_low_threshold(&self, threshold: u32) -> Result<()>;
    /// Returns the bytes handed to [`send`](Self::send)/[`send_text`](Self::send_text)
    /// that SCTP has not yet released (acknowledged or abandoned) — the true
    /// outstanding send-side memory, including bytes still queued in the send
    /// pipeline (unlike `bufferedAmount`, which counts only post-packetization).
    ///
    /// Defaults to `0` so external implementors of this trait keep compiling; the
    /// built-in channel overrides it with the real counter that drives send back-pressure.
    async fn outstanding_bytes(&self) -> Result<usize> {
        Ok(0)
    }
    /// Sends raw binary data on this data channel.
    ///
    /// If a send-buffer limit is configured
    /// ([`PeerConnectionBuilder::with_data_channel_send_buffer_limit`](crate::peer_connection::PeerConnectionBuilder::with_data_channel_send_buffer_limit)),
    /// this **blocks** until the channel's outstanding bytes are below the limit, then
    /// enqueues — mirroring `tokio::mpsc::Sender::send`. With no limit (the default) it
    /// never blocks. Use [`try_send`](Self::try_send) for the non-blocking variant.
    async fn send(&self, data: BytesMut) -> Result<()>;
    /// Sends text data on this data channel.
    ///
    /// Blocking/non-blocking behaviour matches [`send`](Self::send); see there.
    async fn send_text(&self, text: &str) -> Result<()>;
    /// Waits until this channel can accept more data — its outstanding send bytes are
    /// below the configured send-buffer limit.
    ///
    /// Resolves immediately when no limit is configured (the default) or the buffer is
    /// already below it. Returns [`Error::ErrDataChannelClosed`] if the channel or
    /// connection is closing. This is the await-for-capacity primitive that blocking
    /// [`send`](Self::send) uses internally; call it directly to pace your own sends.
    ///
    /// Level-triggered, not a reserved permit: with multiple concurrent senders on one
    /// channel the limit is a soft bound (each admitted sender may add one in-flight
    /// message over it) — the same semantics as `bufferedAmount`-based flow control.
    ///
    /// Defaults to `Ok(())` so external implementors of this trait keep compiling.
    async fn writable(&self) -> Result<()> {
        Ok(())
    }
    /// Non-blocking [`send`](Self::send): enqueues `data` and returns at once, or fails
    /// fast with [`Error::ErrSendBufferFull`] when a send-buffer limit is configured and
    /// this message would push the channel's outstanding bytes past it. Never waits for
    /// capacity. Mirrors `tokio::mpsc::Sender::try_send`.
    ///
    /// `data` is **consumed** on rejection, so retain a clone if you intend to retry.
    ///
    /// Defaults to delegating to [`send`](Self::send) — correct for implementors that do
    /// not impose a blocking limit, since there `send` is already non-blocking.
    async fn try_send(&self, data: BytesMut) -> Result<()> {
        self.send(data).await
    }
    /// Non-blocking [`send_text`](Self::send_text); see [`try_send`](Self::try_send).
    ///
    /// Defaults to delegating to [`send_text`](Self::send_text).
    async fn try_send_text(&self, text: &str) -> Result<()> {
        self.send_text(text).await
    }
    /// Polls for the next event on this data channel.
    async fn poll(&self) -> Option<DataChannelEvent>;
    /// Closes the data channel.
    async fn close(&self) -> Result<()>;
}

/// Events that can occur on a [`DataChannel`].
#[derive(Debug, Clone)]
pub enum DataChannelEvent {
    /// Data channel has opened and is ready to send/receive data.
    ///
    /// This event is fired when the data channel transitions to the "open" state.
    /// Data can now be sent through the channel.
    OnOpen,

    /// An error occurred on the data channel.
    ///
    /// This event is fired when an error is encountered. The channel may still
    /// be usable depending on the error type.
    OnError,

    /// Data channel is closing.
    ///
    /// This event is fired when the channel begins the closing process.
    /// The channel is transitioning to the "closing" state.
    OnClosing,

    /// Data channel has closed.
    ///
    /// This event is fired when the channel is fully closed and no longer usable.
    /// No more data can be sent or received.
    OnClose,

    /// Buffered amount dropped below the low-water mark.
    ///
    /// This event is fired when the amount of buffered outgoing data drops
    /// below the threshold set by `set_buffered_amount_low_threshold()`.
    /// This indicates it's safe to send more data without causing excessive buffering.
    ///
    /// Use this event to implement flow control and prevent memory exhaustion.
    OnBufferedAmountLow,

    /// Buffered amount exceeded the high-water mark (implementation-specific).
    ///
    /// This is a non-standard event that can be used to detect when too much
    /// data is being buffered. Applications should pause sending when this fires.
    OnBufferedAmountHigh,

    /// OnMessage with a binary message arrival over the sctp transport from a remote peer.
    ///
    /// OnMessage can currently receive messages up to 16384 bytes
    /// in size. Check out the detach API if you want to use larger
    /// message sizes. Note that browser support for larger messages
    /// is also limited.
    OnMessage(RTCDataChannelMessage),
}

/// Concrete async data channel implementation (generic over interceptor type).
///
/// This wraps a data channel and provides async send/receive APIs.
pub(crate) struct DataChannelImpl<I = NoopInterceptor>
where
    I: Interceptor,
{
    /// Unique identifier for this data channel
    id: RTCDataChannelId,

    /// Inner PeerConnection Reference
    inner: Arc<PeerConnectionRef<I>>,

    /// event receiver
    evt_rx: Mutex<Receiver<DataChannelEvent>>,
}

impl<I> DataChannelImpl<I>
where
    I: Interceptor,
{
    /// Create a new data channel wrapper
    pub(crate) fn new(
        id: RTCDataChannelId,
        inner: Arc<PeerConnectionRef<I>>,
        evt_rx: Receiver<DataChannelEvent>,
    ) -> Self {
        Self {
            id,
            inner,
            evt_rx: Mutex::new(evt_rx),
        }
    }

    /// Park until the driver signals send-buffer progress, or a 50 ms liveness backstop
    /// fires. Called only on the slow path, when [`writable`](DataChannel::writable) finds
    /// the buffer at/over the limit.
    ///
    /// Holds no lock while waiting: the driver applies SCTP buffer releases (acknowledged
    /// or abandoned bytes) to the per-channel `outstanding_bytes` counter and then wakes
    /// `data_channel_backpressure`, so a blocked sender re-checks and proceeds as soon as
    /// the peer acknowledges data. The 50 ms timeout is a lost-wakeup / liveness backstop
    /// — and, for a peer that never acks, the sender correctly stays blocked (bounding
    /// send-side memory) while still re-checking `closing` so `close()`/`Drop` release it.
    async fn await_send_capacity(&self) {
        futures::select! {
            _ = self.inner.data_channel_backpressure.notified().fuse() => {}
            _ = crate::runtime::sleep(Duration::from_millis(50)).fuse() => {}
        }
    }
}

#[async_trait::async_trait]
impl<I> DataChannel for DataChannelImpl<I>
where
    I: Interceptor + 'static,
{
    /// label represents a label that can be used to distinguish this
    /// DataChannel object from other DataChannel objects. Scripts are
    /// allowed to create multiple DataChannel objects with the same label.
    async fn label(&self) -> Result<String> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .label()
            .to_owned())
    }

    /// Ordered returns true if the DataChannel is ordered, and false if
    /// out-of-order delivery is allowed.
    async fn ordered(&self) -> Result<bool> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .ordered())
    }

    /// max_packet_lifetime represents the length of the time window (msec) during
    /// which transmissions and retransmissions may occur in unreliable mode.
    async fn max_packet_life_time(&self) -> Result<Option<u16>> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .max_packet_life_time())
    }

    /// max_retransmits represents the maximum number of retransmissions that are
    /// attempted in unreliable mode.
    async fn max_retransmits(&self) -> Result<Option<u16>> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .max_retransmits())
    }

    /// protocol represents the name of the sub-protocol used with this
    /// DataChannel.
    async fn protocol(&self) -> Result<String> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .protocol()
            .to_owned())
    }

    /// negotiated represents whether this DataChannel was negotiated by the
    /// application (true), or not (false).
    async fn negotiated(&self) -> Result<bool> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .negotiated())
    }

    /// ID represents the ID for this DataChannel. The value is initially
    /// null, which is what will be returned if the ID was not provided at
    /// channel creation time, and the DTLS role of the SCTP transport has not
    /// yet been negotiated. Otherwise, it will return the ID that was either
    /// selected by the script or generated. After the ID is set to a non-null
    /// value, it will not change.
    fn id(&self) -> RTCDataChannelId {
        self.id
    }

    /// ready_state represents the state of the DataChannel object.
    async fn ready_state(&self) -> Result<RTCDataChannelState> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .ready_state())
    }

    /// buffered_amount_high_threshold represents the threshold at which the
    /// bufferedAmount is considered to be high. When the bufferedAmount increases
    /// from below this threshold to equal or above it, the BufferedAmountHigh
    /// event fires. buffered_amount_high_threshold is initially u32::MAX on each new
    /// DataChannel, but the application may change its value at any time.
    /// The threshold is set to u32::MAX by default.
    async fn buffered_amount_high_threshold(&self) -> Result<u32> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .buffered_amount_high_threshold())
    }

    /// set_buffered_amount_high_threshold sets the threshold at which the
    /// bufferedAmount is considered to be high.
    async fn set_buffered_amount_high_threshold(&self, threshold: u32) -> Result<()> {
        {
            let mut peer_connection = self.inner.core.lock().await;
            peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?
                .set_buffered_amount_high_threshold(threshold);
        }

        self.inner.wake_writes().await;
        Ok(())
    }

    /// buffered_amount_low_threshold represents the threshold at which the
    /// bufferedAmount is considered to be low. When the bufferedAmount decreases
    /// from above this threshold to equal or below it, the BufferedAmountLow
    /// event fires. buffered_amount_low_threshold is initially zero on each new
    /// DataChannel, but the application may change its value at any time.
    /// The threshold is set to 0 by default.
    async fn buffered_amount_low_threshold(&self) -> Result<u32> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .buffered_amount_low_threshold())
    }

    async fn outstanding_bytes(&self) -> Result<usize> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .outstanding_bytes())
    }

    /// set_buffered_amount_low_threshold sets the threshold at which the
    /// bufferedAmount is considered to be low.
    async fn set_buffered_amount_low_threshold(&self, threshold: u32) -> Result<()> {
        {
            let mut peer_connection = self.inner.core.lock().await;
            peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?
                .set_buffered_amount_low_threshold(threshold);
        }

        self.inner.wake_writes().await;
        Ok(())
    }

    /// Send binary data.
    ///
    /// Queues the message and returns; it never waits for the peer to acknowledge. When a
    /// send-buffer limit is configured
    /// ([`PeerConnectionBuilder::with_data_channel_send_buffer_limit`]) it first **blocks**
    /// until the channel's outstanding bytes are below the limit — mirroring
    /// `tokio::mpsc::Sender::send`. With no limit (the default) it never blocks, like the
    /// browser `RTCDataChannel.send()`. For a non-blocking send that fails fast when the
    /// buffer is full, use [`try_send`](Self::try_send).
    ///
    /// # Errors
    ///
    /// - [`Error::ErrDataChannelClosed`] if the channel/connection is closed or closing —
    ///   including a caller blocked awaiting capacity when `close()`/`Drop` runs.
    ///
    /// [`PeerConnectionBuilder::with_data_channel_send_buffer_limit`]: crate::peer_connection::PeerConnectionBuilder::with_data_channel_send_buffer_limit
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use bytes::BytesMut;
    /// # use webrtc::error::Result;
    /// # use webrtc::data_channel::DataChannel;
    /// # use std::sync::Arc;
    /// # async fn example(dc: Arc<dyn DataChannel>) -> Result<()> {
    /// dc.send(BytesMut::from(&b"Hello, WebRTC!"[..])).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn send(&self, data: BytesMut) -> Result<()> {
        // Await capacity, then enqueue. `writable()` is a no-op unless a send-buffer limit
        // is configured; when one is, it blocks until the channel's outstanding bytes fall
        // below it. It also returns ErrDataChannelClosed terminally on a closing connection,
        // so a caller blocked on a stalled peer is released by close()/Drop rather than
        // buffering unboundedly. Mirrors tokio::mpsc::Sender::send.
        self.writable().await?;
        {
            let mut peer_connection = self.inner.core.lock().await;
            let mut dc = peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?;
            dc.send(data)?;
        }

        // Wake the driver so it flushes SCTP output (poll_write) and checks
        // for newly generated events (e.g. OnBufferedAmountHigh).
        self.inner.wake_writes().await;
        Ok(())
    }

    /// Send text data.
    ///
    /// Blocking/non-blocking behaviour and error contract match [`send`](Self::send): with a
    /// configured send-buffer limit it blocks until the buffer is below the limit, and
    /// returns [`Error::ErrDataChannelClosed`] on a closing/closed channel. Use
    /// [`try_send_text`](Self::try_send_text) for the non-blocking variant.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use webrtc::error::Result;
    /// # use webrtc::data_channel::DataChannel;
    /// # use std::sync::Arc;
    /// # async fn example(dc: Arc<dyn DataChannel>) -> Result<()> {
    /// dc.send_text("Hello, WebRTC!").await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn send_text(&self, text: &str) -> Result<()> {
        // Await capacity, then enqueue — see `send` for the rationale.
        self.writable().await?;
        {
            let mut peer_connection = self.inner.core.lock().await;
            let mut dc = peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?;
            dc.send_text(text)?;
        }

        self.inner.wake_writes().await;
        Ok(())
    }

    async fn writable(&self) -> Result<()> {
        // Terminal on a closing/closed connection: once close()/Drop stops the driver it no
        // longer drains outstanding_bytes, so waiting could never make progress. Return
        // ErrDataChannelClosed at once (mirrors send()-after-close in the browser).
        if self.inner.closing.load(Ordering::Acquire) {
            return Err(Error::ErrDataChannelClosed);
        }
        let limit = self.inner.data_channel_send_buffer_limit;
        // Unbounded (the default): every send is admitted, so the channel is always
        // writable — skip locking the core entirely. This is what keeps `send`/`send_text`
        // non-blocking and lock-once on the default path.
        if limit == usize::MAX {
            return Ok(());
        }
        loop {
            if self.inner.closing.load(Ordering::Acquire) {
                return Err(Error::ErrDataChannelClosed);
            }
            {
                let mut peer_connection = self.inner.core.lock().await;
                let outstanding = peer_connection
                    .data_channel(self.id)
                    .ok_or(Error::ErrDataChannelClosed)?
                    .outstanding_bytes();
                // Below the limit ⇒ writable. An empty buffer (`0 < limit`) also passes, so
                // a lone message larger than the limit can still be sent rather than
                // blocking forever.
                if outstanding < limit {
                    return Ok(());
                }
            } // release the core lock before parking
            self.await_send_capacity().await;
        }
    }

    async fn try_send(&self, data: BytesMut) -> Result<()> {
        // Non-blocking peer of `send`: same terminal-on-close guard, but instead of awaiting
        // capacity it fails fast. Back-pressure is folded into the send's own core-lock so
        // the fast path takes the lock exactly ONCE: peek outstanding_bytes and enqueue
        // atomically.
        if self.inner.closing.load(Ordering::Acquire) {
            return Err(Error::ErrDataChannelClosed);
        }
        let limit = self.inner.data_channel_send_buffer_limit;
        {
            let mut peer_connection = self.inner.core.lock().await;
            let mut dc = peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?;
            let outstanding = dc.outstanding_bytes();
            // Reject once the message would push outstanding bytes past the limit — but
            // always admit onto an empty buffer, so a lone message larger than the limit can
            // still be sent. saturating_add so an unbounded (`usize::MAX`) limit can't
            // overflow.
            if outstanding != 0 && outstanding.saturating_add(data.len()) > limit {
                return Err(Error::ErrSendBufferFull);
            }
            dc.send(data)?;
        }

        self.inner.wake_writes().await;
        Ok(())
    }

    async fn try_send_text(&self, text: &str) -> Result<()> {
        // Non-blocking peer of `send_text`; see `try_send`.
        if self.inner.closing.load(Ordering::Acquire) {
            return Err(Error::ErrDataChannelClosed);
        }
        let limit = self.inner.data_channel_send_buffer_limit;
        {
            let mut peer_connection = self.inner.core.lock().await;
            let mut dc = peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?;
            let outstanding = dc.outstanding_bytes();
            if outstanding != 0 && outstanding.saturating_add(text.len()) > limit {
                return Err(Error::ErrSendBufferFull);
            }
            dc.send_text(text)?;
        }

        self.inner.wake_writes().await;
        Ok(())
    }

    async fn poll(&self) -> Option<DataChannelEvent> {
        self.evt_rx.lock().await.recv().await
    }

    async fn close(&self) -> Result<()> {
        {
            let mut peer_connection = self.inner.core.lock().await;
            peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?
                .close()?;
        }

        self.inner.wake_writes().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::block_on;
    use std::sync::atomic::AtomicUsize;

    /// A minimal external `DataChannel` that overrides only `send`/`send_text` (to record that
    /// they ran) and leaves `outstanding_bytes`, `writable`, `try_send` and `try_send_text` to
    /// the trait's **default bodies**.
    ///
    /// This locks in the backward-compatibility contract the send back-pressure work relies on:
    /// a downstream `impl DataChannel` that predates these methods must keep compiling and get
    /// sensible defaults — `outstanding_bytes` reports `0` ("no back-pressure signal"),
    /// `writable` resolves `Ok(())`, and `try_send`/`try_send_text` delegate to `send`/`send_text`
    /// (correct where `send` is already non-blocking). Every other method is unreachable here and
    /// left `unimplemented!()`.
    #[derive(Default)]
    struct DefaultDataChannel {
        sends: AtomicUsize,
        text_sends: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl DataChannel for DefaultDataChannel {
        async fn label(&self) -> Result<String> {
            unimplemented!()
        }
        async fn ordered(&self) -> Result<bool> {
            unimplemented!()
        }
        async fn max_packet_life_time(&self) -> Result<Option<u16>> {
            unimplemented!()
        }
        async fn max_retransmits(&self) -> Result<Option<u16>> {
            unimplemented!()
        }
        async fn protocol(&self) -> Result<String> {
            unimplemented!()
        }
        async fn negotiated(&self) -> Result<bool> {
            unimplemented!()
        }
        fn id(&self) -> RTCDataChannelId {
            unimplemented!()
        }
        async fn ready_state(&self) -> Result<RTCDataChannelState> {
            unimplemented!()
        }
        async fn buffered_amount_high_threshold(&self) -> Result<u32> {
            unimplemented!()
        }
        async fn set_buffered_amount_high_threshold(&self, _threshold: u32) -> Result<()> {
            unimplemented!()
        }
        async fn buffered_amount_low_threshold(&self) -> Result<u32> {
            unimplemented!()
        }
        async fn set_buffered_amount_low_threshold(&self, _threshold: u32) -> Result<()> {
            unimplemented!()
        }
        // outstanding_bytes(), writable(), try_send(), try_send_text(): deliberately NOT
        // overridden — exercising the trait defaults is the whole point of this test.
        async fn send(&self, _data: BytesMut) -> Result<()> {
            self.sends.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        async fn send_text(&self, _text: &str) -> Result<()> {
            self.text_sends.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        async fn poll(&self) -> Option<DataChannelEvent> {
            unimplemented!()
        }
        async fn close(&self) -> Result<()> {
            unimplemented!()
        }
    }

    #[test]
    fn outstanding_bytes_trait_default_is_zero() {
        let dc = DefaultDataChannel::default();
        let n =
            block_on(dc.outstanding_bytes()).expect("default outstanding_bytes() must return Ok");
        assert_eq!(
            n, 0,
            "the DataChannel::outstanding_bytes default must report 0 outstanding bytes"
        );
    }

    #[test]
    fn writable_trait_default_is_ok() {
        let dc = DefaultDataChannel::default();
        block_on(dc.writable()).expect("the DataChannel::writable default must resolve Ok(())");
    }

    #[test]
    fn try_send_trait_default_delegates_to_send() {
        let dc = DefaultDataChannel::default();
        block_on(dc.try_send(BytesMut::new())).expect("default try_send() must return Ok");
        assert_eq!(
            dc.sends.load(Ordering::Relaxed),
            1,
            "the DataChannel::try_send default must delegate to send()"
        );
        assert_eq!(dc.text_sends.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn try_send_text_trait_default_delegates_to_send_text() {
        let dc = DefaultDataChannel::default();
        block_on(dc.try_send_text("x")).expect("default try_send_text() must return Ok");
        assert_eq!(
            dc.text_sends.load(Ordering::Relaxed),
            1,
            "the DataChannel::try_send_text default must delegate to send_text()"
        );
        assert_eq!(dc.sends.load(Ordering::Relaxed), 0);
    }
}
