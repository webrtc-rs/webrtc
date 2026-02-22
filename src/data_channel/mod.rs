//! Async DataChannel API

use crate::peer_connection::{MessageInner, PeerConnectionRef};
use crate::runtime::{Mutex, Receiver};
use bytes::BytesMut;
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::shared::error::{Error, Result};
use std::sync::Arc;

pub use rtc::data_channel::{
    RTCDataChannelId, RTCDataChannelInit, RTCDataChannelMessage, RTCDataChannelState,
};

/// Object-safe trait exposing all public DataChannel operations.
///
/// This allows `on_data_channel` in `PeerConnectionEventHandler` to receive a
/// `Arc<dyn DataChannelExt>` without the event handler trait itself needing to
/// be generic over the interceptor type `I`.
#[async_trait::async_trait]
pub trait DataChannel: Send + Sync + 'static {
    async fn label(&self) -> Result<String>;
    async fn ordered(&self) -> Result<bool>;
    async fn max_packet_life_time(&self) -> Option<u16>;
    async fn max_retransmits(&self) -> Option<u16>;
    async fn protocol(&self) -> Result<String>;
    async fn negotiated(&self) -> Result<bool>;
    fn id(&self) -> RTCDataChannelId;
    async fn ready_state(&self) -> Result<RTCDataChannelState>;
    async fn buffered_amount_high_threshold(&self) -> Result<u32>;
    async fn set_buffered_amount_high_threshold(&self, threshold: u32) -> Result<()>;
    async fn buffered_amount_low_threshold(&self) -> Result<u32>;
    async fn set_buffered_amount_low_threshold(&self, threshold: u32) -> Result<()>;
    async fn send(&self, data: BytesMut) -> Result<()>;
    async fn send_text(&self, text: &str) -> Result<()>;
    async fn poll(&self) -> Option<DataChannelEvent>;
    async fn close(&self) -> Result<()>;
}

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
    async fn max_packet_life_time(&self) -> Option<u16> {
        let mut peer_connection = self.inner.core.lock().await;
        peer_connection
            .data_channel(self.id)?
            .max_packet_life_time()
    }

    /// max_retransmits represents the maximum number of retransmissions that are
    /// attempted in unreliable mode.
    async fn max_retransmits(&self) -> Option<u16> {
        let mut peer_connection = self.inner.core.lock().await;
        peer_connection.data_channel(self.id)?.max_retransmits()
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

        self.inner
            .msg_tx
            .try_send(MessageInner::WriteNotify)
            .map_err(|e| Error::Other(format!("{:?}", e)))
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

        self.inner
            .msg_tx
            .try_send(MessageInner::WriteNotify)
            .map_err(|e| Error::Other(format!("{:?}", e)))
    }

    /// Send binary data
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
        {
            let mut peer_connection = self.inner.core.lock().await;
            peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?
                .send(data)?;
        }

        // Wake the driver so it flushes SCTP output (poll_write) and checks
        // for newly generated events (e.g. OnBufferedAmountHigh).
        self.inner
            .msg_tx
            .try_send(MessageInner::WriteNotify)
            .map_err(|e| Error::Other(format!("{:?}", e)))
    }

    /// Send text data
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
        {
            let mut peer_connection = self.inner.core.lock().await;
            peer_connection
                .data_channel(self.id)
                .ok_or(Error::ErrDataChannelClosed)?
                .send_text(text)?;
        }

        self.inner
            .msg_tx
            .try_send(MessageInner::WriteNotify)
            .map_err(|e| Error::Other(format!("{:?}", e)))
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

        self.inner
            .msg_tx
            .try_send(MessageInner::WriteNotify)
            .map_err(|e| Error::Other(format!("{:?}", e)))
    }
}
