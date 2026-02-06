//! Async peer connection wrapper

use super::ice_gatherer::RTCIceGatherOptions;
use super::*;
use crate::data_channel::DataChannel;
use crate::ice_gatherer::RTCIceGatherer;
use crate::media_track::TrackRemote;
use crate::peer_connection_driver::PeerConnectionDriver;
use crate::runtime::{Mutex, Receiver, Sender, channel};
use crate::runtime::{Runtime, default_runtime};
use log::error;
use rtc::data_channel::{RTCDataChannelId, RTCDataChannelInit, RTCDataChannelMessage};
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::peer_connection::RTCPeerConnection;
use rtc::peer_connection::configuration::{RTCAnswerOptions, RTCOfferOptions};
use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::rtp_transceiver::{RTCRtpReceiverId, RTCRtpSenderId};
use rtc::sansio::Protocol;
use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;

/// Trait for handling peer connection events asynchronously
///
/// This trait defines callbacks that are invoked when various WebRTC events occur.
/// All methods are async and have default no-op implementations.
///
/// # Example
///
/// ```no_run
/// use webrtc::peer_connection::*;
/// use webrtc::RTCPeerConnectionIceEvent;
///
/// #[derive(Clone)]
/// struct MyHandler;
///
/// #[async_trait::async_trait]
/// impl PeerConnectionEventHandler for MyHandler {
///     async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
///         println!("New ICE candidate: {:?}", event.candidate);
///         // Send to remote peer via signaling
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait PeerConnectionEventHandler: Send + Sync + 'static {
    /// Called when negotiation is needed
    async fn on_negotiation_needed(&self) {}

    /// Called when a new ICE candidate is available
    async fn on_ice_candidate(&self, _event: RTCPeerConnectionIceEvent) {}

    /// Called when an ICE candidate error occurs
    async fn on_ice_candidate_error(&self, _event: RTCPeerConnectionIceErrorEvent) {}

    /// Called when the signaling state changes
    async fn on_signaling_state_change(&self, _state: RTCSignalingState) {}

    /// Called when the ICE connection state changes
    async fn on_ice_connection_state_change(&self, _state: RTCIceConnectionState) {}

    /// Called when the ICE gathering state changes
    async fn on_ice_gathering_state_change(&self, _state: RTCIceGatheringState) {}

    /// Called when the peer connection state changes
    async fn on_connection_state_change(&self, _state: RTCPeerConnectionState) {}

    /// Called when a remote peer creates a data channel
    async fn on_data_channel_open(&self, _data_channel: Arc<DataChannel>) {}

    /// Called for data channel lifecycle events
    async fn on_data_channel(&self, _event: RTCDataChannelEvent) {}

    /// Called when a remote track is received
    async fn on_track_open(&self, _track: Arc<TrackRemote>) {}

    /// Called for track lifecycle events
    async fn on_track(&self, _event: RTCTrackEvent) {}
}

/// Unified inner message type for the peer connection driver
#[derive(Debug)]
pub(crate) enum MessageInner {
    /// Outgoing data channel message
    DataChannelMessage(RTCDataChannelId, RTCDataChannelMessage),
    /// Outgoing RTP packet from local track
    SenderRtp(RTCRtpSenderId, rtc::rtp::Packet),
    /// Outgoing RTCP packets from sender
    SenderRtcp(RTCRtpSenderId, Vec<Box<dyn rtc::rtcp::Packet>>),
    /// Outgoing RTCP packets from receiver
    ReceiverRtcp(RTCRtpReceiverId, Vec<Box<dyn rtc::rtcp::Packet>>),
    /// New local ICE candidate
    LocalIceCandidate(RTCIceCandidateInit),
    Close,
}

pub struct PeerConnectionBuilder<A: ToSocketAddrs> {
    config: RTCConfiguration,
    runtime: Option<Arc<dyn Runtime>>,
    handler: Option<Arc<dyn PeerConnectionEventHandler>>,
    udp_addrs: Vec<A>,
    tcp_addrs: Vec<A>,
}

impl<A: ToSocketAddrs> PeerConnectionBuilder<A> {
    pub fn new(config: RTCConfiguration) -> Self {
        Self {
            config,
            runtime: None,
            handler: None,
            udp_addrs: vec![],
            tcp_addrs: vec![],
        }
    }

    pub fn with_runtime(mut self, runtime: Arc<dyn Runtime>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn with_handler(mut self, handler: Arc<dyn PeerConnectionEventHandler>) -> Self {
        self.handler = Some(handler);
        self
    }

    pub fn with_udp_addrs(mut self, udp_addrs: Vec<A>) -> Self {
        self.udp_addrs = udp_addrs;
        self
    }

    pub fn with_tcp_addrs(mut self, tcp_addrs: Vec<A>) -> Self {
        self.tcp_addrs = tcp_addrs;
        self
    }

    pub async fn build(self) -> Result<PeerConnection> {
        let runtime = if let Some(runtime) = self.runtime {
            runtime
        } else {
            default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?
        };
        PeerConnection::new(
            self.config,
            runtime,
            self.handler
                .ok_or_else(|| std::io::Error::other("no event handler found"))?,
            self.udp_addrs,
            self.tcp_addrs,
        )
        .await
    }
}

/// Async-friendly peer connection
///
/// This wraps the Sans-I/O `RTCPeerConnection` from the rtc crate and provides
/// an async API.
pub struct PeerConnection<I = NoopInterceptor>
where
    I: Interceptor,
{
    pub(crate) inner: Arc<PeerConnectionRef<I>>,
    driver_handle: Option<runtime::JoinHandle>,
}

impl<I> Drop for PeerConnection<I>
where
    I: Interceptor,
{
    fn drop(&mut self) {
        // Abort the driver task when PeerConnection is dropped
        if let Some(handle) = &self.driver_handle {
            handle.abort();
        }
    }
}

pub(crate) struct PeerConnectionRef<I = NoopInterceptor>
where
    I: Interceptor,
{
    /// The sans-I/O peer connection core (uses default NoopInterceptor)
    pub(crate) core: Mutex<RTCPeerConnection<I>>,
    /// Runtime for async operations
    pub(crate) runtime: Arc<dyn Runtime>,
    /// Event handler
    pub(crate) handler: Arc<dyn PeerConnectionEventHandler>,
    /// ICE gatherer for managing ICE candidate gathering
    pub(crate) ice_gatherer: RTCIceGatherer,
    /// Local socket address (set after bind)
    pub(crate) local_addr: Mutex<Option<SocketAddr>>,
    /// Data channels  
    pub(crate) data_channels: Mutex<HashMap<RTCDataChannelId, Arc<DataChannel>>>,
    /// Data channel message senders (for incoming messages from network)
    pub(crate) data_channel_rxs: Mutex<HashMap<RTCDataChannelId, Sender<RTCDataChannelMessage>>>,
    /// Remote tracks (incoming media)
    pub(crate) remote_tracks: Mutex<HashMap<RTCRtpReceiverId, Arc<TrackRemote>>>,
    /// RTP packet senders for remote tracks
    pub(crate) track_rxs: Mutex<HashMap<RTCRtpReceiverId, Sender<rtc::rtp::Packet>>>,
    /// Unified channel for all outgoing messages
    pub(crate) msg_tx: Sender<MessageInner>,
    /// Unified channel receiver for all outgoing messages (taken by driver)
    pub(crate) msg_rx: Mutex<Option<Receiver<MessageInner>>>,
}

impl<I> PeerConnection<I>
where
    I: Interceptor + 'static,
{
    /// Create a new peer connection with a custom runtime
    async fn new<A: ToSocketAddrs>(
        config: RTCConfiguration<I>,
        runtime: Arc<dyn Runtime>,
        handler: Arc<dyn PeerConnectionEventHandler>,
        udp_addrs: Vec<A>,
        _tcp_addrs: Vec<A>,
    ) -> Result<Self> {
        let ice_servers = config.ice_servers().to_vec();

        let core = RTCPeerConnection::new(config)?;

        // Create unified channel for all outgoing messages
        let (outgoing_tx, outgoing_rx) = channel();

        // Create ICE gatherer with servers from config
        let ice_gatherer = RTCIceGatherer::new(
            outgoing_tx.clone(),
            RTCIceGatherOptions {
                ice_servers,
                ice_gather_policy: RTCIceTransportPolicy::All,
            },
        );

        let mut async_udp_sockets = vec![];
        for addr in udp_addrs {
            let socket = std::net::UdpSocket::bind(addr)?;
            socket.set_nonblocking(true)?;
            let async_udp_socket = runtime.wrap_udp_socket(socket)?;
            async_udp_sockets.push(async_udp_socket);
        }

        let mut peer_connection = Self {
            inner: Arc::new(PeerConnectionRef {
                core: Mutex::new(core),
                runtime: runtime.clone(),
                handler,
                ice_gatherer,
                local_addr: Mutex::new(None),
                data_channels: Mutex::new(HashMap::new()),
                data_channel_rxs: Mutex::new(HashMap::new()),
                remote_tracks: Mutex::new(HashMap::new()),
                track_rxs: Mutex::new(HashMap::new()),
                msg_tx: outgoing_tx,
                msg_rx: Mutex::new(Some(outgoing_rx)),
            }),
            driver_handle: None,
        };

        let mut driver =
            PeerConnectionDriver::new(peer_connection.inner.clone(), async_udp_sockets).await?;
        peer_connection.driver_handle = Some(runtime.spawn(Box::pin(async move {
            if let Err(e) = driver.run().await {
                error!("I/O error: {}", e);
            }
        })));

        Ok(peer_connection)
    }

    /// Close the peer connection
    pub async fn close(&mut self) -> Result<()> {
        {
            let mut core = self.inner.core.lock().await;
            core.close()?;
        }
        self.inner
            .msg_tx
            .try_send(MessageInner::Close)
            .map_err(|e| Error::Other(format!("{:?}", e)))?;

        self.driver_handle.take();

        Ok(())
    }

    /// Create an SDP offer
    pub async fn create_offer(
        &self,
        options: Option<RTCOfferOptions>,
    ) -> Result<RTCSessionDescription> {
        let mut core = self.inner.core.lock().await;
        core.create_offer(options)
    }

    /// Create an SDP answer
    pub async fn create_answer(
        &self,
        options: Option<RTCAnswerOptions>,
    ) -> Result<RTCSessionDescription> {
        let mut core = self.inner.core.lock().await;
        core.create_answer(options)
    }

    /// Set the local description
    pub async fn set_local_description(&self, desc: RTCSessionDescription) -> Result<()> {
        {
            let mut core = self.inner.core.lock().await;
            core.set_local_description(desc)?;
        }

        let local_addr_opt = *self.inner.local_addr.lock().await;
        if let Some(local_addr) = local_addr_opt {
            self.inner
                .ice_gatherer
                .gather(Arc::clone(&self.inner.runtime), local_addr)
                .await;
        }

        Ok(())
    }

    /// Get the local description
    pub async fn local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.local_description().cloned()
    }

    /// Get current local description
    pub async fn current_local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.current_local_description().cloned()
    }

    /// Get pending local description
    pub async fn pending_local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.pending_local_description().cloned()
    }

    /// Returns whether the remote peer supports trickle ICE.
    pub async fn can_trickle_ice_candidates(&self) -> Option<bool> {
        let core = self.inner.core.lock().await;
        core.can_trickle_ice_candidates()
    }

    /// Set the remote description
    pub async fn set_remote_description(&self, desc: RTCSessionDescription) -> Result<()> {
        let mut core = self.inner.core.lock().await;
        core.set_remote_description(desc)?;
        Ok(())
    }

    /// Get the remote description
    pub async fn remote_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.remote_description().cloned()
    }

    /// Get current remote description
    pub async fn current_remote_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.current_remote_description().cloned()
    }

    /// Get pending remote description
    pub async fn pending_remote_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.pending_remote_description().cloned()
    }

    /// Add an ICE candidate received from the remote peer
    ///
    /// This method adds a remote ICE candidate received through the signaling channel.
    /// The remote description must be set before calling this method.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use webrtc::peer_connection::*;
    /// use webrtc::RTCIceCandidateInit;
    /// # use webrtc::Result;
    /// # use std::sync::Arc;
    /// # struct Handler;
    /// # #[async_trait::async_trait]
    /// # impl PeerConnectionEventHandler for Handler {}
    /// # async fn example(pc: PeerConnection) -> Result<()> {
    /// // Receive candidate from signaling channel
    /// let candidate_init = RTCIceCandidateInit {
    ///     candidate: "candidate:1 1 UDP 2130706431 192.168.1.100 54321 typ host".to_string(),
    ///     sdp_mid: Some("0".to_string()),
    ///     sdp_mline_index: Some(0),
    ///     username_fragment: None,
    ///     url: None,
    /// };
    ///
    /// pc.add_ice_candidate(candidate_init).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<()> {
        let mut core = self.inner.core.lock().await;
        core.add_remote_candidate(candidate)?;
        Ok(())
    }

    /// Restart ICE
    ///
    /// Triggers an ICE restart. The next call to `create_offer()` will generate
    /// an offer with new ICE credentials, causing a full ICE restart.
    ///
    /// This is useful when the ICE connection has failed or when network conditions
    /// have changed (e.g., switching networks on a mobile device).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use webrtc::peer_connection::PeerConnection;
    /// # use webrtc::Result;
    /// # async fn example(pc: PeerConnection) -> Result<()> {
    /// // Trigger ICE restart on connection failure
    /// pc.restart_ice().await?;
    ///
    /// // Create new offer with new ICE credentials
    /// let offer = pc.create_offer(None).await?;
    /// pc.set_local_description(offer.clone()).await?;
    /// // Send offer to remote peer through signaling
    /// # Ok(())
    /// # }
    /// ```
    pub async fn restart_ice(&self) -> Result<()> {
        {
            let mut core = self.inner.core.lock().await;
            core.restart_ice();
        }

        //TODO: ICE Gatherer?
        let local_addr_opt = *self.inner.local_addr.lock().await;
        if let Some(local_addr) = local_addr_opt {
            self.inner
                .ice_gatherer
                .gather(Arc::clone(&self.inner.runtime), local_addr)
                .await;
        }

        Ok(())
    }

    /// set_configuration updates the configuration of this PeerConnection object.
    pub async fn set_configuration(&self, configuration: RTCConfiguration<I>) -> Result<()> {
        let mut core = self.inner.core.lock().await;
        core.set_configuration(configuration)
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
    /// # use webrtc::RTCDataChannelInit;
    /// # use webrtc::RTCConfigurationBuilder;
    /// # use webrtc::Result;
    /// # use std::sync::Arc;
    /// # #[derive(Clone)]
    /// # struct MyHandler;
    /// # #[async_trait::async_trait]
    /// # impl PeerConnectionEventHandler for MyHandler {}
    /// # async fn example() -> Result<()> {
    /// let config = RTCConfigurationBuilder::new().build();
    /// let handler = Arc::new(MyHandler);
    /// let pc = PeerConnectionBuilder::new(config).with_handler(handler).with_udp_addrs(vec!["127.0.0.1:0"]).build().await?;
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
        options: Option<RTCDataChannelInit>,
    ) -> Result<Arc<DataChannel>> {
        let label = label.into();

        // Create the data channel via the core
        let channel_id = {
            let mut core = self.inner.core.lock().await;
            let rtc_dc = core.create_data_channel(&label, options)?;
            rtc_dc.id()
        };

        // Create a channel for receiving messages for this data channel
        let (dc_tx, dc_rx) = channel();

        // Create our async wrapper
        let dc = Arc::new(DataChannel::new(
            channel_id,
            label,
            self.inner.msg_tx.clone(),
            dc_rx,
        ));

        // Store in the maps
        self.inner
            .data_channels
            .lock()
            .await
            .insert(channel_id, dc.clone());
        self.inner
            .data_channel_rxs
            .lock()
            .await
            .insert(channel_id, dc_tx);

        Ok(dc)
    }
}
