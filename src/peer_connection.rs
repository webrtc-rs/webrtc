//! Async peer connection wrapper

use super::ice_gatherer::RTCIceGatherOptions;
use super::*;
use crate::data_channel::{DataChannel, DataChannelEvent, DataChannelImpl};
use crate::ice_gatherer::RTCIceGatherer;
use crate::media_track::{TrackLocal, TrackRemote};
use crate::peer_connection_driver::PeerConnectionDriver;
use crate::rtp_transceiver::{RtpReceiver, RtpSender, RtpTransceiver};
use crate::runtime::{Mutex, Sender, channel};
use crate::runtime::{Runtime, default_runtime};
use log::error;
use rtc::data_channel::{RTCDataChannelId, RTCDataChannelInit};
use rtc::interceptor::{Interceptor, NoopInterceptor, Registry};
use rtc::peer_connection::configuration::{RTCAnswerOptions, RTCOfferOptions};
use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::peer_connection::{RTCPeerConnection, RTCPeerConnectionBuilder};
use rtc::rtp_transceiver::rtp_sender::RtpCodecKind;
use rtc::sansio::Protocol;
use rtc::statistics::StatsSelector;
use rtc::statistics::report::RTCStatsReport;
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Instant;

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
    async fn on_track(&self, _track: Arc<dyn TrackRemote>) {}
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

    pub async fn build(self) -> Result<impl PeerConnection> {
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

        PeerConnectionImpl::new(
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

/// Object-safe trait exposing all public PeerConnection operations.
///
/// `PeerConnectionBuilder::build()` returns `Arc<dyn PeerConnection>`, hiding the
/// generic interceptor type so callers can store and share connections easily.
///
/// # Example
///
/// ```no_run
/// use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
/// use webrtc::RTCConfigurationBuilder;
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct MyHandler;
/// #[async_trait::async_trait]
/// impl PeerConnectionEventHandler for MyHandler {}
///
/// # async fn example() -> webrtc::Result<()> {
/// let pc = PeerConnectionBuilder::new()
///     .with_handler(Arc::new(MyHandler))
///     .with_udp_addrs(vec!["127.0.0.1:0"])
///     .build()
///     .await?;
///
/// let offer = pc.create_offer(None).await?;
/// # Ok(())
/// # }
/// ```
#[async_trait::async_trait]
pub trait PeerConnection: Send + Sync + 'static {
    /// Close the peer connection
    async fn close(&self) -> Result<()>;
    /// Create an SDP offer
    async fn create_offer(&self, options: Option<RTCOfferOptions>)
    -> Result<RTCSessionDescription>;
    /// Create an SDP answer
    async fn create_answer(
        &self,
        options: Option<RTCAnswerOptions>,
    ) -> Result<RTCSessionDescription>;
    /// Set the local description
    async fn set_local_description(&self, desc: RTCSessionDescription) -> Result<()>;
    /// Get the local description
    async fn local_description(&self) -> Option<RTCSessionDescription>;
    /// Get current local description
    async fn current_local_description(&self) -> Option<RTCSessionDescription>;
    /// Get pending local description
    async fn pending_local_description(&self) -> Option<RTCSessionDescription>;
    /// Returns whether the remote peer supports trickle ICE.
    async fn can_trickle_ice_candidates(&self) -> Option<bool>;
    /// Set the remote description
    async fn set_remote_description(&self, desc: RTCSessionDescription) -> Result<()>;
    /// Get the remote description
    async fn remote_description(&self) -> Option<RTCSessionDescription>;
    /// Get current remote description
    async fn current_remote_description(&self) -> Option<RTCSessionDescription>;
    /// Get pending remote description
    async fn pending_remote_description(&self) -> Option<RTCSessionDescription>;
    /// Add a remote ICE candidate
    async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<()>;
    /// Trigger an ICE restart
    async fn restart_ice(&self) -> Result<()>;
    /// Get the current configuration
    async fn get_configuration(&self) -> RTCConfiguration;
    /// Update the configuration
    async fn set_configuration(&self, configuration: RTCConfiguration) -> Result<()>;
    /// Create a data channel
    async fn create_data_channel(
        &self,
        label: &str,
        options: Option<RTCDataChannelInit>,
    ) -> Result<Arc<dyn DataChannel>>;
    /// Get the list of rtp sender
    async fn get_senders(&self) -> Vec<Arc<dyn RtpSender>>;
    /// Get the list of rtp receiver
    async fn get_receivers(&self) -> Vec<Arc<dyn RtpReceiver>>;
    /// Get the list of rtp transceiver
    async fn get_transceivers(&self) -> Vec<Arc<dyn RtpTransceiver>>;
    /// Add a Track to the PeerConnection
    async fn add_track(&self, track: Arc<dyn TrackLocal>) -> Result<Arc<dyn RtpSender>>;
    /// Remove a Track from the PeerConnection
    async fn remove_track(&self, sender: &Arc<dyn RtpSender>) -> Result<()>;
    /// Create a new RtpTransceiver(SendRecv or SendOnly) and add it to the set of transceivers
    async fn add_transceiver_from_track(
        &self,
        track: Arc<dyn TrackLocal>,
        init: Option<RTCRtpTransceiverInit>,
    ) -> Result<Arc<dyn RtpTransceiver>>;
    /// Create a new RtpTransceiver and adds it to the set of transceivers
    async fn add_transceiver_from_kind(
        &self,
        kind: RtpCodecKind,
        init: Option<RTCRtpTransceiverInit>,
    ) -> Result<Arc<dyn RtpTransceiver>>;
    /// Get a snapshot of accumulated statistics.
    async fn get_stats(&self, now: Instant, selector: StatsSelector) -> RTCStatsReport;
}

/// Concrete async peer connection implementation (generic over interceptor type).
///
/// Not exposed directly — obtained as `Arc<dyn PeerConnection>` from `PeerConnectionBuilder::build()`.
pub(crate) struct PeerConnectionImpl<I = NoopInterceptor>
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

impl<I> PeerConnectionImpl<I>
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
        let peer_connection = Self {
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
        let driver_handle = runtime.spawn(Box::pin(async move {
            if let Err(e) = driver.event_loop(msg_rx).await {
                error!("I/O error: {}", e);
            }
        }));
        *peer_connection.driver_handle.lock().await = Some(driver_handle);

        Ok(peer_connection)
    }
}

#[async_trait::async_trait]
impl<I> PeerConnection for PeerConnectionImpl<I>
where
    I: Interceptor + 'static,
{
    async fn close(&self) -> Result<()> {
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

    async fn create_offer(
        &self,
        options: Option<RTCOfferOptions>,
    ) -> Result<RTCSessionDescription> {
        let mut core = self.inner.core.lock().await;
        core.create_offer(options)
    }

    async fn create_answer(
        &self,
        options: Option<RTCAnswerOptions>,
    ) -> Result<RTCSessionDescription> {
        let mut core = self.inner.core.lock().await;
        core.create_answer(options)
    }

    async fn set_local_description(&self, desc: RTCSessionDescription) -> Result<()> {
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

    async fn local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.local_description()
    }

    async fn current_local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.current_local_description()
    }

    async fn pending_local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.pending_local_description()
    }

    async fn can_trickle_ice_candidates(&self) -> Option<bool> {
        let core = self.inner.core.lock().await;
        core.can_trickle_ice_candidates()
    }

    async fn set_remote_description(&self, desc: RTCSessionDescription) -> Result<()> {
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

    async fn remote_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.remote_description().cloned()
    }

    async fn current_remote_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.current_remote_description().cloned()
    }

    async fn pending_remote_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.pending_remote_description().cloned()
    }

    async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<()> {
        let mut core = self.inner.core.lock().await;
        core.add_remote_candidate(candidate)?;
        Ok(())
    }

    async fn restart_ice(&self) -> Result<()> {
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

    async fn get_configuration(&self) -> RTCConfiguration {
        let core = self.inner.core.lock().await;
        core.get_configuration().clone()
    }

    async fn set_configuration(&self, configuration: RTCConfiguration) -> Result<()> {
        let mut core = self.inner.core.lock().await;
        core.set_configuration(configuration)
    }

    async fn create_data_channel(
        &self,
        label: &str,
        options: Option<RTCDataChannelInit>,
    ) -> Result<Arc<dyn DataChannel>> {
        // Create the data channel via the core
        let channel_id = {
            let mut core = self.inner.core.lock().await;
            let rtc_dc = core.create_data_channel(label, options)?;
            rtc_dc.id()
        };

        let (evt_tx, evt_rx) = channel();
        {
            let mut data_channels = self.inner.data_channels.lock().await;
            data_channels.insert(channel_id, evt_tx);
        }

        Ok(Arc::new(DataChannelImpl::new(
            channel_id,
            self.inner.clone(),
            evt_rx,
        )))
    }

    /// Get the list of rtp sender
    async fn get_senders(&self) -> Vec<Arc<dyn RtpSender>> {
        //TODO:
        vec![]
    }

    /// Get the list of rtp receiver
    async fn get_receivers(&self) -> Vec<Arc<dyn RtpReceiver>> {
        //TODO:
        vec![]
    }

    /// Get the list of rtp transceiver
    async fn get_transceivers(&self) -> Vec<Arc<dyn RtpTransceiver>> {
        //TODO:
        vec![]
    }

    /// Add a Track to the PeerConnection
    async fn add_track(&self, _track: Arc<dyn TrackLocal>) -> Result<Arc<dyn RtpSender>> {
        //TODO:
        Err(Error::ErrRTPSenderNotExisted)
    }

    /// Remove a Track from the PeerConnection
    async fn remove_track(&self, _sender: &Arc<dyn RtpSender>) -> Result<()> {
        //TODO:
        Ok(())
    }

    /// Create a new RtpTransceiver(SendRecv or SendOnly) and add it to the set of transceivers
    async fn add_transceiver_from_track(
        &self,
        _track: Arc<dyn TrackLocal>,
        _init: Option<RTCRtpTransceiverInit>,
    ) -> Result<Arc<dyn RtpTransceiver>> {
        //TODO:
        Err(Error::ErrRTPSenderTrackNil)
    }

    /// Create a new RtpTransceiver and adds it to the set of transceivers
    async fn add_transceiver_from_kind(
        &self,
        _kind: RtpCodecKind,
        _init: Option<RTCRtpTransceiverInit>,
    ) -> Result<Arc<dyn RtpTransceiver>> {
        //TODO:
        Err(Error::ErrRTPSenderTrackNil)
    }

    /// Get a snapshot of accumulated statistics.
    async fn get_stats(&self, now: Instant, selector: StatsSelector) -> RTCStatsReport {
        let mut core = self.inner.core.lock().await;
        core.get_stats(now, selector)
    }
}
