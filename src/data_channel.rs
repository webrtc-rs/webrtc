//! Async DataChannel implementation

use crate::peer_connection::{MessageInner, PeerConnectionRef};
use crate::{Error, Result};
use bytes::BytesMut;
use rtc::data_channel::{RTCDataChannelId, RTCDataChannelMessage, RTCDataChannelState};
use rtc::interceptor::{Interceptor, NoopInterceptor};
use std::sync::Arc;

/// Async-friendly data channel
///
/// This wraps a data channel and provides async send/receive APIs.
pub struct DataChannel<I = NoopInterceptor>
where
    I: Interceptor,
{
    /// Unique identifier for this data channel
    id: RTCDataChannelId,

    /// Inner PeerConnection Reference
    inner: Arc<PeerConnectionRef<I>>,
}

impl<I> DataChannel<I>
where
    I: Interceptor,
{
    /// Create a new data channel wrapper
    pub(crate) fn new(id: RTCDataChannelId, inner: Arc<PeerConnectionRef<I>>) -> Self {
        Self { id, inner }
    }

    /// label represents a label that can be used to distinguish this
    /// DataChannel object from other DataChannel objects. Scripts are
    /// allowed to create multiple DataChannel objects with the same label.
    pub async fn label(&self) -> Result<String> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .label()
            .to_owned())
    }

    /// Ordered returns true if the DataChannel is ordered, and false if
    /// out-of-order delivery is allowed.
    pub async fn ordered(&self) -> Result<bool> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .ordered())
    }

    /// max_packet_lifetime represents the length of the time window (msec) during
    /// which transmissions and retransmissions may occur in unreliable mode.
    pub async fn max_packet_life_time(&self) -> Option<u16> {
        let mut peer_connection = self.inner.core.lock().await;
        peer_connection
            .data_channel(self.id)?
            .max_packet_life_time()
    }

    /// max_retransmits represents the maximum number of retransmissions that are
    /// attempted in unreliable mode.
    pub async fn max_retransmits(&self) -> Option<u16> {
        let mut peer_connection = self.inner.core.lock().await;
        peer_connection.data_channel(self.id)?.max_retransmits()
    }

    /// protocol represents the name of the sub-protocol used with this
    /// DataChannel.
    pub async fn protocol(&self) -> Result<String> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .protocol()
            .to_owned())
    }

    /// negotiated represents whether this DataChannel was negotiated by the
    /// application (true), or not (false).
    pub async fn negotiated(&self) -> Result<bool> {
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
    pub fn id(&self) -> RTCDataChannelId {
        self.id
    }

    /// ready_state represents the state of the DataChannel object.
    pub async fn ready_state(&self) -> Result<RTCDataChannelState> {
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
    pub async fn buffered_amount_high_threshold(&self) -> Result<u32> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .buffered_amount_high_threshold())
    }

    /// set_buffered_amount_high_threshold sets the threshold at which the
    /// bufferedAmount is considered to be high.
    pub async fn set_buffered_amount_high_threshold(&mut self, threshold: u32) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;
        peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .set_buffered_amount_high_threshold(threshold);
        Ok(())
    }

    /// buffered_amount_low_threshold represents the threshold at which the
    /// bufferedAmount is considered to be low. When the bufferedAmount decreases
    /// from above this threshold to equal or below it, the BufferedAmountLow
    /// event fires. buffered_amount_low_threshold is initially zero on each new
    /// DataChannel, but the application may change its value at any time.
    /// The threshold is set to 0 by default.
    pub async fn buffered_amount_low_threshold(&self) -> Result<u32> {
        let mut peer_connection = self.inner.core.lock().await;
        Ok(peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .buffered_amount_low_threshold())
    }

    /// set_buffered_amount_low_threshold sets the threshold at which the
    /// bufferedAmount is considered to be low.
    pub async fn set_buffered_amount_low_threshold(&mut self, threshold: u32) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;
        peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .set_buffered_amount_low_threshold(threshold);
        Ok(())
    }

    /// Send binary data
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use bytes::BytesMut;
    /// # use webrtc::Result;
    /// # async fn example(dc: webrtc::data_channel::DataChannel) -> Result<()> {
    /// dc.send(BytesMut::from(&b"Hello, WebRTC!"[..])).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send(&self, data: BytesMut) -> Result<()> {
        let message = RTCDataChannelMessage {
            is_string: false,
            data,
        };

        self.inner
            .msg_tx
            .try_send(MessageInner::DataChannelMessage(self.id, message))
            .map_err(|e| Error::Other(format!("{:?}", e)))?;

        Ok(())
    }

    /// Send text data
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use webrtc::Result;
    /// # async fn example(dc: webrtc::data_channel::DataChannel) -> Result<()> {
    /// dc.send_text("Hello, WebRTC!").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_text(&self, text: impl Into<String>) -> Result<()> {
        let text = text.into();
        let data = BytesMut::from(text.as_bytes());

        let message = RTCDataChannelMessage {
            is_string: true,
            data,
        };

        self.inner
            .msg_tx
            .try_send(MessageInner::DataChannelMessage(self.id, message))
            .map_err(|e| Error::Other(format!("{:?}", e)))?;

        Ok(())
    }

    pub async fn close(&mut self) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;
        peer_connection
            .data_channel(self.id)
            .ok_or(Error::ErrDataChannelClosed)?
            .close()
    }
}
