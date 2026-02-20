//! Async peer connection wrapper

use super::ice_gatherer::RTCIceGatherOptions;
use super::*;
use crate::data_channel::{DataChannel, DataChannelEvent, DataChannelInternal};
use crate::ice_gatherer::RTCIceGatherer;
use crate::media_track::TrackRemote;
use crate::peer_connection_driver::PeerConnectionDriver;
use crate::runtime::{Mutex, Sender, channel};
use crate::runtime::{Runtime, default_runtime};
use log::error;
use rtc::data_channel::{RTCDataChannelId, RTCDataChannelInit};
use rtc::interceptor::{Interceptor, NoopInterceptor, Registry};
use rtc::peer_connection::configuration::{RTCAnswerOptions, RTCOfferOptions};
use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::peer_connection::{RTCPeerConnection, RTCPeerConnectionBuilder};
use rtc::sansio::Protocol;
use std::collections::HashMap;
use std::net::ToSocketAddrs;
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
    async fn on_data_channel(&self, _data_channel: Arc<dyn DataChannel>) {}

    /// Called when a remote track is received
    async fn on_track(&self, _track: Arc<TrackRemote>) {}
}

/// Unified inner message type for the peer connection driver
#[derive(Debug)]
pub(crate) enum MessageInner {
    // Outgoing RTP packet from local track
    //SenderRtp(RTCRtpSenderId, rtc::rtp::Packet),
    // Outgoing RTCP packets from sender
    //SenderRtcp(RTCRtpSenderId, Vec<Box<dyn rtc::rtcp::Packet>>),
    // Outgoing RTCP packets from receiver
    //ReceiverRtcp(RTCRtpReceiverId, Vec<Box<dyn rtc::rtcp::Packet>>),
    WriteNotify,
    IceGathering,
    Close,
}

pub struct PeerConnectionBuilder<A: ToSocketAddrs, I = NoopInterceptor>
where
    I: Interceptor,
{
    builder: RTCPeerConnectionBuilder<I>,
    runtime: Option<Arc<dyn Runtime>>,
    handler: Option<Arc<dyn PeerConnectionEventHandler>>,
    udp_addrs: Vec<A>,
    tcp_addrs: Vec<A>,
}

impl<A: ToSocketAddrs> Default for PeerConnectionBuilder<A, NoopInterceptor> {
    fn default() -> Self {
        Self {
            builder: RTCPeerConnectionBuilder::new(),
            runtime: None,
            handler: None,
            udp_addrs: vec![],
            tcp_addrs: vec![],
        }
    }
}

impl<A: ToSocketAddrs> PeerConnectionBuilder<A, NoopInterceptor> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<A: ToSocketAddrs, I> PeerConnectionBuilder<A, I>
where
    I: Interceptor,
{
    pub fn with_configuration(mut self, configuration: RTCConfiguration) -> Self {
        self.builder = self.builder.with_configuration(configuration);
        self
    }

    pub fn with_media_engine(mut self, media_engine: MediaEngine) -> Self {
        self.builder = self.builder.with_media_engine(media_engine);
        self
    }

    pub fn with_setting_engine(mut self, setting_engine: SettingEngine) -> Self {
        self.builder = self.builder.with_setting_engine(setting_engine);
        self
    }

    pub fn with_interceptor_registry<P>(
        self,
        interceptor_registry: Registry<P>,
    ) -> PeerConnectionBuilder<A, P>
    where
        P: Interceptor,
    {
        PeerConnectionBuilder {
            builder: self.builder.with_interceptor_registry(interceptor_registry),
            runtime: self.runtime,
            handler: self.handler,
            udp_addrs: self.udp_addrs,
            tcp_addrs: self.tcp_addrs,
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

    pub async fn build(self) -> Result<PeerConnection<I>> {
        let runtime = if let Some(runtime) = self.runtime {
            runtime
        } else {
            default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?
        };

        let core = self.builder.build()?;
        let configuration = core.get_configuration();

        let opts = RTCIceGatherOptions {
            ice_servers: configuration.ice_servers().to_vec(),
            ice_gather_policy: configuration.ice_transport_policy(),
        };

        PeerConnection::new(
            core,
            runtime,
            self.handler
                .ok_or_else(|| std::io::Error::other("no event handler found"))?,
            opts,
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
    inner: Arc<PeerConnectionRef<I>>,
    driver_handle: Mutex<Option<runtime::JoinHandle>>,
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
    pub(crate) data_channels: Mutex<HashMap<RTCDataChannelId, Sender<DataChannelEvent>>>,
    /// Unified channel for all outgoing messages
    pub(crate) msg_tx: Sender<MessageInner>,
}

impl<I> PeerConnection<I>
where
    I: Interceptor,
{
    /// Create a new peer connection with a custom runtime
    async fn new<A: ToSocketAddrs>(
        core: RTCPeerConnection<I>,
        runtime: Arc<dyn Runtime>,
        handler: Arc<dyn PeerConnectionEventHandler>,
        opts: RTCIceGatherOptions,
        udp_addrs: Vec<A>,
        _tcp_addrs: Vec<A>,
    ) -> Result<Self> {
        let mut local_addrs = vec![];
        let mut async_udp_sockets = HashMap::new();
        for addr in udp_addrs {
            let socket = std::net::UdpSocket::bind(addr)?;
            socket.set_nonblocking(true)?;
            let local_addr = socket.local_addr()?;
            let async_udp_socket = runtime.wrap_udp_socket(socket)?;
            if async_udp_sockets
                .insert(local_addr, async_udp_socket)
                .is_none()
            {
                local_addrs.push(local_addr);
            }
        }

        let (msg_tx, msg_rx) = channel();
        let mut peer_connection = Self {
            inner: Arc::new(PeerConnectionRef {
                core: Mutex::new(core),
                runtime: runtime.clone(),
                data_channels: Mutex::new(HashMap::new()),
                handler,
                msg_tx,
            }),
            driver_handle: Mutex::new(None),
        };

        let ice_gatherer = RTCIceGatherer::new(local_addrs, opts);
        let mut driver = PeerConnectionDriver::new(
            peer_connection.inner.clone(),
            ice_gatherer,
            async_udp_sockets,
        )
        .await?;
        peer_connection.driver_handle = Mutex::new(Some(runtime.spawn(Box::pin(async move {
            if let Err(e) = driver.event_loop(msg_rx).await {
                error!("I/O error: {}", e);
            }
        }))));

        Ok(peer_connection)
    }

    /// Close the peer connection
    pub async fn close(&self) -> Result<()> {
        {
            let mut core = self.inner.core.lock().await;
            core.close()?;
        }
        self.inner
            .msg_tx
            .try_send(MessageInner::Close)
            .map_err(|e| Error::Other(format!("{:?}", e)))?;

        {
            let mut driver_handle = self.driver_handle.lock().await;
            if let Some(driver_handle) = driver_handle.take() {
                driver_handle.abort();
            }
        }

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

        // Wake the driver with MessageInner::IceGathering. Without this
        // notify the driver would sleep until its previous (possibly 1-day default)
        // timer expired and never send STUN binding requests.
        self.inner
            .msg_tx
            .send(MessageInner::IceGathering)
            .await
            .map_err(|e| Error::Other(format!("{:?}", e)))?;
        Ok(())
    }

    /// Get the local description
    pub async fn local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.local_description()
    }

    /// Get current local description
    pub async fn current_local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.current_local_description()
    }

    /// Get pending local description
    pub async fn pending_local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.pending_local_description()
    }

    /// Returns whether the remote peer supports trickle ICE.
    pub async fn can_trickle_ice_candidates(&self) -> Option<bool> {
        let core = self.inner.core.lock().await;
        core.can_trickle_ice_candidates()
    }

    /// Set the remote description
    pub async fn set_remote_description(&self, desc: RTCSessionDescription) -> Result<()> {
        {
            let mut core = self.inner.core.lock().await;
            core.set_remote_description(desc)?;
        }
        // Wake the driver so it re-polls its timeout. When both local and remote
        // descriptions are set, set_remote_description triggers start_transports
        // internally, which arms the ICE connectivity-check timer. Without this
        // notify the driver would sleep until its previous (possibly 1-day default)
        // timer expired and never send the initial STUN binding requests.
        let _ = self.inner.msg_tx.try_send(MessageInner::WriteNotify);
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

        self.inner
            .msg_tx
            .send(MessageInner::IceGathering)
            .await
            .map_err(|e| Error::Other(format!("{:?}", e)))?;
        Ok(())
    }

    /// set_configuration updates the configuration of this PeerConnection object.
    pub async fn get_configuration(&self) -> RTCConfiguration {
        let core = self.inner.core.lock().await;
        core.get_configuration().clone()
    }

    /// set_configuration updates the configuration of this PeerConnection object.
    pub async fn set_configuration(&self, configuration: RTCConfiguration) -> Result<()> {
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
    /// let pc = PeerConnectionBuilder::new().with_configuration(config).with_handler(handler).with_udp_addrs(vec!["127.0.0.1:0"]).build().await?;
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
    ) -> Result<Arc<dyn DataChannel>> {
        let label = label.into();

        // Create the data channel via the core
        let channel_id = {
            let mut core = self.inner.core.lock().await;
            let rtc_dc = core.create_data_channel(&label, options)?;
            rtc_dc.id()
        };

        let (evt_tx, evt_rx) = channel();
        {
            let mut data_channels = self.inner.data_channels.lock().await;
            data_channels.insert(channel_id, evt_tx);
        }

        // Create our async wrapper
        let dc = Arc::new(DataChannelInternal::new(
            channel_id,
            self.inner.clone(),
            evt_rx,
        ));

        Ok(dc)
    }
}
