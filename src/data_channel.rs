//! Async DataChannel implementation

use crate::peer_connection::MessageInner;
use crate::runtime::{Mutex, Receiver, Sender};
use crate::{Error, Result};
use bytes::BytesMut;
use rtc::data_channel::{RTCDataChannelId, RTCDataChannelMessage, RTCDataChannelState};
use std::sync::Arc;

/// Async-friendly data channel
///
/// This wraps a data channel and provides async send/receive APIs.
pub struct DataChannel {
    /// Unique identifier for this data channel
    pub id: RTCDataChannelId,

    /// Label for this data channel
    pub label: String,

    /// Current state
    state: Arc<Mutex<RTCDataChannelState>>,

    /// Channel for sending messages to the driver
    tx: Sender<MessageInner>,

    /// Channel for receiving messages from the driver
    rx: Arc<Mutex<Receiver<RTCDataChannelMessage>>>,
}

impl DataChannel {
    /// Create a new data channel wrapper
    pub(crate) fn new(
        id: RTCDataChannelId,
        label: String,
        tx: Sender<MessageInner>,
        rx: Receiver<RTCDataChannelMessage>,
    ) -> Self {
        Self {
            id,
            label,
            state: Arc::new(Mutex::new(RTCDataChannelState::Connecting)),
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Send binary data
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use bytes::BytesMut;
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

        self.tx
            .try_send(MessageInner::DataChannelMessage(self.id, message))
            .map_err(|e| Error::Other(format!("{:?}", e)))?;

        Ok(())
    }

    /// Send text data
    ///
    /// # Example
    ///
    /// ```no_run
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

        self.tx
            .try_send(MessageInner::DataChannelMessage(self.id, message))
            .map_err(|e| Error::Other(format!("{:?}", e)))?;

        Ok(())
    }

    /// Receive a message
    ///
    /// Returns `None` when the channel is closed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example(dc: webrtc::data_channel::DataChannel) {
    /// while let Some(msg) = dc.recv().await {
    ///     if msg.is_string {
    ///         let text = String::from_utf8_lossy(&msg.data);
    ///         println!("Received text: {}", text);
    ///     } else {
    ///         println!("Received binary: {} bytes", msg.data.len());
    ///     }
    /// }
    /// # }
    /// ```
    pub async fn recv(&self) -> Option<RTCDataChannelMessage> {
        let mut rx = self.rx.lock().await;
        rx.recv().await
    }

    /// Get the current state of the data channel
    pub async fn state(&self) -> RTCDataChannelState {
        *self.state.lock().await
    }

    /// Update the state (called by driver)
    pub(crate) async fn set_state(&self, new_state: RTCDataChannelState) {
        *self.state.lock().await = new_state;
    }
}
