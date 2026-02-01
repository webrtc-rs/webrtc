//! Async peer connection wrapper

use super::*;
use crate::data_channel::DataChannel;
use crate::runtime::Runtime;
use rtc::data_channel::{RTCDataChannelId, RTCDataChannelMessage};
use rtc::peer_connection::RTCPeerConnection;
use rtc::peer_connection::configuration::{RTCAnswerOptions, RTCOfferOptions};
use rtc::sansio::Protocol;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Async-friendly peer connection
///
/// This wraps the Sans-I/O `RTCPeerConnection` from the rtc crate and provides
/// an async API.
pub struct PeerConnection {
    pub(crate) inner: Arc<PeerConnectionInner>,
}

pub(crate) struct PeerConnectionInner {
    /// The sans-I/O peer connection core (uses default NoopInterceptor)
    pub(crate) core: Mutex<RTCPeerConnection>,
    /// Runtime for async operations
    pub(crate) runtime: Arc<dyn Runtime>,
    /// Event handler
    pub(crate) handler: Arc<dyn PeerConnectionEventHandler>,
    /// Data channels  
    pub(crate) data_channels: Mutex<HashMap<RTCDataChannelId, Arc<DataChannel>>>,
    /// Data channel message senders (for incoming messages from network)
    pub(crate) data_channel_rxs:
        Mutex<HashMap<RTCDataChannelId, mpsc::UnboundedSender<RTCDataChannelMessage>>>,
    /// Channel for outgoing data channel messages
    pub(crate) data_tx: mpsc::UnboundedSender<crate::data_channel::OutgoingMessage>,
    /// Channel for receiving outgoing data channel messages (taken by driver)
    pub(crate) data_rx:
        Mutex<Option<mpsc::UnboundedReceiver<crate::data_channel::OutgoingMessage>>>,
}

// Safety: we protect it with Mutex to make it Send + Sync
unsafe impl Send for PeerConnectionInner {}
unsafe impl Sync for PeerConnectionInner {}

impl PeerConnection {
    /// Create a new peer connection with a custom runtime
    pub fn new_with_runtime(
        runtime: Arc<dyn Runtime>,
        config: RTCConfiguration,
        handler: Arc<dyn PeerConnectionEventHandler>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let core = RTCPeerConnection::new(config)?;
        let (data_tx, data_rx) = mpsc::unbounded_channel();

        Ok(Self {
            inner: Arc::new(PeerConnectionInner {
                core: Mutex::new(core),
                runtime,
                handler,
                data_channels: Mutex::new(HashMap::new()),
                data_channel_rxs: Mutex::new(HashMap::new()),
                data_tx,
                data_rx: Mutex::new(Some(data_rx)),
            }),
        })
    }

    /// Create a new peer connection with the default runtime
    #[cfg(any(feature = "runtime-tokio", feature = "runtime-smol"))]
    pub fn new(
        config: RTCConfiguration,
        handler: Arc<dyn PeerConnectionEventHandler>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = crate::runtime::default_runtime().ok_or("No default runtime available")?;
        Self::new_with_runtime(runtime, config, handler)
    }

    /// Create an SDP offer
    pub fn create_offer(
        &self,
        options: Option<RTCOfferOptions>,
    ) -> Result<RTCSessionDescription, Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().unwrap();
        Ok(core.create_offer(options)?)
    }

    /// Create an SDP answer
    pub fn create_answer(
        &self,
        options: Option<RTCAnswerOptions>,
    ) -> Result<RTCSessionDescription, Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().unwrap();
        Ok(core.create_answer(options)?)
    }

    /// Set the local description
    pub fn set_local_description(
        &self,
        desc: RTCSessionDescription,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().unwrap();
        core.set_local_description(desc)?;
        Ok(())
    }

    /// Set the remote description
    pub fn set_remote_description(
        &self,
        desc: RTCSessionDescription,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().unwrap();
        core.set_remote_description(desc)?;
        Ok(())
    }

    /// Get the local description
    pub fn local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().unwrap();
        core.local_description().cloned()
    }

    /// Get the remote description
    pub fn remote_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().unwrap();
        core.remote_description().cloned()
    }

    /// Close the peer connection
    pub fn close(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().unwrap();
        core.close()?;
        Ok(())
    }

    /// Create a data channel
    ///
    /// Creates a new data channel with the specified label and configuration.
    /// The data channel will be in the "connecting" state until the peer connection
    /// is established.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use webrtc::peer_connection::*;
    /// # use webrtc::data_channel::RTCDataChannelInit;
    /// # use std::sync::Arc;
    /// # #[derive(Clone)]
    /// # struct MyHandler;
    /// # #[async_trait::async_trait]
    /// # impl PeerConnectionEventHandler for MyHandler {}
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = RTCConfigurationBuilder::new().build();
    /// let handler = Arc::new(MyHandler);
    /// let pc = PeerConnection::new(config, handler)?;
    ///
    /// // Create a data channel
    /// let dc = pc.create_data_channel("my-channel", None).await?;
    ///
    /// // Send messages
    /// dc.send_text("Hello!").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_data_channel(
        &self,
        label: impl Into<String>,
        options: Option<crate::data_channel::RTCDataChannelInit>,
    ) -> Result<Arc<DataChannel>, Box<dyn std::error::Error>> {
        let label = label.into();

        // Create the data channel via the core
        let channel_id = {
            let mut core = self.inner.core.lock().unwrap();
            let rtc_dc = core.create_data_channel(&label, options)?;
            rtc_dc.id()
        };

        // Create a channel for receiving messages for this data channel
        let (dc_tx, dc_rx) = mpsc::unbounded_channel();

        // Create our async wrapper
        let dc = Arc::new(DataChannel::new(
            channel_id,
            label,
            self.inner.data_tx.clone(),
            dc_rx,
        ));

        // Store in the maps
        self.inner
            .data_channels
            .lock()
            .unwrap()
            .insert(channel_id, dc.clone());
        self.inner
            .data_channel_rxs
            .lock()
            .unwrap()
            .insert(channel_id, dc_tx);

        Ok(dc)
    }

    /// Bind a UDP socket and create a driver for this peer connection
    ///
    /// The driver must be spawned or awaited to handle I/O and events.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use webrtc::peer_connection::*;
    /// # use std::sync::Arc;
    /// # use std::net::SocketAddr;
    /// # #[derive(Clone)]
    /// # struct MyHandler;
    /// # #[async_trait::async_trait]
    /// # impl PeerConnectionEventHandler for MyHandler {}
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = RTCConfigurationBuilder::new().build();
    /// let handler = Arc::new(MyHandler);
    /// let pc = PeerConnection::new(config, handler)?;
    ///
    /// // Bind to any available port
    /// let addr: SocketAddr = "0.0.0.0:0".parse()?;
    /// let driver = pc.bind(addr).await?;
    ///
    /// // Spawn the driver in the background
    /// tokio::spawn(async move {
    ///     if let Err(e) = driver.await {
    ///         eprintln!("Driver error: {}", e);
    ///     }
    /// });
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bind(
        &self,
        addr: impl Into<SocketAddr>,
    ) -> Result<PeerConnectionDriver, Box<dyn std::error::Error>> {
        let addr = addr.into();
        let socket = std::net::UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;

        let async_socket = self.inner.runtime.wrap_udp_socket(socket)?;
        let driver = PeerConnectionDriver::new(self.inner.clone(), async_socket)?;

        Ok(driver)
    }
}
