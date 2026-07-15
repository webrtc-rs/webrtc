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
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::shared::error::{Error, Result};
use std::sync::Arc;
use std::sync::atomic::Ordering;

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
    async fn send(&self, data: BytesMut) -> Result<()>;
    /// Sends text data on this data channel.
    async fn send_text(&self, text: &str) -> Result<()>;
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
    /// Non-blocking, like the browser `RTCDataChannel.send()`: it queues the message and
    /// returns; it never waits for the peer to acknowledge. To bound send-side memory it
    /// applies a **non-blocking** back-pressure gate — see *Errors*.
    ///
    /// # Errors
    ///
    /// - [`Error::ErrSendBufferFull`] if this message would push the channel's outstanding
    ///   send bytes (handed to `send`/`send_text` but not yet acknowledged or abandoned by
    ///   SCTP) past the configured send-buffer limit
    ///   ([`PeerConnectionBuilder::with_data_channel_send_buffer_limit`], default 16 MiB;
    ///   set `0` to disable). This mirrors the browser's "throw when the send queue is
    ///   full". It is **retryable**, but only *after* the buffer drains — an application
    ///   should pace on the [`OnBufferedAmountLow`](DataChannelEvent::OnBufferedAmountLow)
    ///   event (see `examples/data-channels-flow-control`) or poll
    ///   [`outstanding_bytes`](Self::outstanding_bytes) rather than spin. Note `data` is
    ///   **consumed** on rejection, so retain a clone if you intend to retry.
    /// - [`Error::ErrDataChannelClosed`] if the channel/connection is closed or closing.
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
        // A closing/closed connection must fail TERMINALLY, not with a retryable
        // ErrSendBufferFull: once close()/Drop stops the driver it no longer drains
        // outstanding_bytes, and the channel is not removed from the core map, so an app
        // retrying on ErrSendBufferFull would loop forever. Return ErrDataChannelClosed at
        // once (mirrors send()-after-close in the browser).
        if self.inner.closing.load(Ordering::Acquire) {
            return Err(Error::ErrDataChannelClosed);
        }
        // Non-blocking send back-pressure, folded into the send's own core-lock so the fast
        // path takes the lock exactly ONCE: check the channel's outstanding-bytes counter
        // and enqueue atomically. A caller that outruns the drain (a slow reactor, or a
        // slow/malicious peer advertising a tiny receive window) is rejected with
        // ErrSendBufferFull rather than growing send-side memory without bound — never
        // blocking. The limit is applied per-channel and configured connection-wide via
        // PeerConnectionBuilder::with_data_channel_send_buffer_limit (usize::MAX = unbounded).
        let limit = self.inner.data_channel_send_buffer_limit;
        {
            let mut peer_connection = self.inner.core.lock().await;
            let mut dc = peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?;
            let outstanding = dc.outstanding_bytes();
            // Reject once the message would push outstanding bytes past the limit — but
            // always admit onto an empty buffer, so a lone message larger than the limit
            // can still be sent rather than being permanently rejected. saturating_add so
            // an unbounded (`usize::MAX`) limit can't overflow.
            if outstanding != 0 && outstanding.saturating_add(data.len()) > limit {
                return Err(Error::ErrSendBufferFull);
            }
            dc.send(data)?;
        }

        // Wake the driver so it flushes SCTP output (poll_write) and checks
        // for newly generated events (e.g. OnBufferedAmountHigh).
        self.inner.wake_writes().await;
        Ok(())
    }

    /// Send text data.
    ///
    /// Non-blocking. Same back-pressure and error contract as [`send`](Self::send):
    /// returns [`Error::ErrSendBufferFull`] (retryable, after the buffer drains) once the
    /// message would exceed the configured send-buffer limit, and
    /// [`Error::ErrDataChannelClosed`] on a closing/closed channel.
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
        // Same single-lock, non-blocking back-pressure as `send` (see there), including the
        // terminal-error-on-close guard so a retry loop can't livelock past teardown.
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
