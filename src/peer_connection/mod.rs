//! PeerConnection API
//!
//! This module provides the core [`PeerConnection`] trait and its builder [`PeerConnectionBuilder`],
//! which are used to establish peer-to-peer connections for media and data streaming.
//!
//! # Architecture
//!
//! A `PeerConnection` consists of two main parts:
//! 1. **`PeerConnection`**: The user-facing API handle. All operations (e.g., `create_offer`,
//!    `add_track`, `create_data_channel`) are asynchronous and communicate with a background driver.
//! 2. **`PeerConnectionDriver`**: A background event loop spawned automatically when building a
//!    connection. It drives the underlying Sans-I/O `rtc` protocol core, manages network sockets
//!    (UDP/TCP), handles timeouts, and dispatches events.
//!
//! # Examples
//!
//! ## Creating a Peer Connection
//!
//! ```no_run
//! use webrtc::peer_connection::{
//!     PeerConnectionBuilder, PeerConnectionEventHandler,
//!     RTCConfigurationBuilder, RTCIceServer,
//! };
//! use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct MyHandler;
//!
//! #[async_trait::async_trait]
//! impl PeerConnectionEventHandler for MyHandler {
//!     // Implement event handlers...
//! }
//!
//! # async fn example() -> webrtc::error::Result<()> {
//! let pc = PeerConnectionBuilder::new()
//!     .with_configuration(
//!         RTCConfigurationBuilder::default()
//!             .with_ice_servers(vec![RTCIceServer {
//!                 urls: vec!["stun:stun.l.google.com:19302".to_owned()],
//!                 ..Default::default()
//!             }])
//!             .build(),
//!     )
//!     .with_handler(Arc::new(MyHandler))
//!     .with_udp_addrs(vec!["0.0.0.0:0"])
//!     .build()
//!     .await?;
//! # Ok(())
//! # }
//! ```

pub(crate) mod driver;
pub(crate) mod transports;

use log::error;
use std::collections::{HashMap, HashSet};
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Instant;

use crate::data_channel::{DataChannel, DataChannelEvent, DataChannelImpl};
use crate::media_stream::{track_local::TrackLocal, track_remote::TrackRemote};
use crate::rtp_transceiver::{RtpReceiver, RtpSender, RtpTransceiver, RtpTransceiverImpl};
use crate::runtime::{JoinHandle, Runtime, default_runtime};
use crate::runtime::{Mutex, Sender, channel};
use std::sync::atomic::AtomicBool;

use driver::{
    DATA_CHANNEL_EVENT_CHANNEL_CAPACITY, PEER_CONNECTION_DRIVER_EVENT_CHANNEL_CAPACITY,
    PeerConnectionDriver,
};
use transports::stun_gatherer::RTCStunGatherer;
use transports::turn_relayer::RTCTurnRelayer;

use rtc::data_channel::{RTCDataChannelId, RTCDataChannelInit};
use rtc::ice::mdns::MulticastDnsMode;
use rtc::mdns::MulticastSocket;
use rtc::peer_connection::RTCPeerConnectionBuilder;
use rtc::peer_connection::configuration::{RTCAnswerOptions, RTCOfferOptions};
use rtc::rtp_transceiver::rtp_sender::RtpCodecKind;
use rtc::rtp_transceiver::{RTCRtpTransceiverId, RTCRtpTransceiverInit};
use rtc::sansio::Protocol;
use rtc::shared::error::{Error, Result};
use rtc::statistics::StatsSelector;
use rtc::statistics::report::RTCStatsReport;

use crate::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use crate::media_stream::track_remote::TrackRemoteEvent;
use crate::peer_connection::driver::PeerConnectionDriverEvent;
use crate::rtp_transceiver::rtp_sender::RtpSenderImpl;
pub use rtc::interceptor::{Interceptor, NoopInterceptor, Registry};
use rtc::media_stream::MediaStreamTrackId;
pub use rtc::peer_connection::{
    RTCPeerConnection,
    certificate::RTCCertificate,
    configuration::{
        RTCBundlePolicy, RTCConfiguration, RTCConfigurationBuilder, RTCIceServer,
        RTCIceTransportPolicy, RTCRtcpMuxPolicy, interceptor_registry::*,
        media_engine::MediaEngine, setting_engine::SettingEngine,
    },
    event::{
        RTCDataChannelEvent, RTCPeerConnectionEvent, RTCPeerConnectionIceErrorEvent,
        RTCPeerConnectionIceEvent, RTCTrackEvent,
    },
    sdp::{RTCSdpType, RTCSessionDescription},
    state::{
        RTCIceConnectionState, RTCIceGatheringState, RTCPeerConnectionState, RTCSignalingState,
    },
    transport::{RTCIceCandidate, RTCIceCandidateInit, RTCIceCandidateType, RTCIceProtocol},
};

/// Trait for handling peer connection events asynchronously
///
/// This trait defines callbacks that are invoked when various WebRTC events occur.
/// All methods are async and have default no-op implementations.
///
/// # Example
///
/// ```no_run
/// use webrtc::peer_connection::{PeerConnectionEventHandler, RTCPeerConnectionIceEvent};
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

/// Builder for constructing a [`PeerConnection`].
///
/// Configures the configuration, media engine, setting engine, interceptor registry,
/// event handler, async runtime, and local socket addresses.
pub struct PeerConnectionBuilder<A: ToSocketAddrs, I = NoopInterceptor>
where
    I: Interceptor,
{
    builder: RTCPeerConnectionBuilder<I>,
    runtime: Option<Arc<dyn Runtime>>,
    handler: Option<Arc<dyn PeerConnectionEventHandler>>,
    mdns_mode: MulticastDnsMode,
    udp_addrs: Vec<A>,
    tcp_addrs: Vec<A>,
}

impl<A: ToSocketAddrs> Default for PeerConnectionBuilder<A, NoopInterceptor> {
    fn default() -> Self {
        Self {
            builder: RTCPeerConnectionBuilder::new(),
            runtime: None,
            handler: None,
            mdns_mode: MulticastDnsMode::Disabled,
            udp_addrs: vec![],
            tcp_addrs: vec![],
        }
    }
}

impl<A: ToSocketAddrs> PeerConnectionBuilder<A, NoopInterceptor> {
    /// Creates a new `PeerConnectionBuilder`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<A: ToSocketAddrs, I> PeerConnectionBuilder<A, I>
where
    I: Interceptor,
{
    /// Configures the builder with the specified WebRTC [`RTCConfiguration`].
    pub fn with_configuration(mut self, configuration: RTCConfiguration) -> Self {
        self.builder = self.builder.with_configuration(configuration);
        self
    }

    /// Configures the builder with the specified [`MediaEngine`].
    pub fn with_media_engine(mut self, media_engine: MediaEngine) -> Self {
        self.builder = self.builder.with_media_engine(media_engine);
        self
    }

    /// Configures the builder with the specified [`SettingEngine`].
    pub fn with_setting_engine(mut self, setting_engine: SettingEngine) -> Self {
        self.mdns_mode = setting_engine.multicast_dns().mode;
        self.builder = self.builder.with_setting_engine(setting_engine);
        self
    }

    /// Configures the builder with the specified interceptor [`Registry`].
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
            mdns_mode: self.mdns_mode,
            udp_addrs: self.udp_addrs,
            tcp_addrs: self.tcp_addrs,
        }
    }

    /// Configures the builder with the specified async [`Runtime`].
    pub fn with_runtime(mut self, runtime: Arc<dyn Runtime>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    /// Configures the builder with the specified [`PeerConnectionEventHandler`].
    pub fn with_handler(mut self, handler: Arc<dyn PeerConnectionEventHandler>) -> Self {
        self.handler = Some(handler);
        self
    }

    /// Configures the builder with the local UDP socket addresses to bind.
    pub fn with_udp_addrs(mut self, udp_addrs: Vec<A>) -> Self {
        self.udp_addrs = udp_addrs;
        self
    }

    /// Configures the builder with the local TCP socket addresses to bind.
    pub fn with_tcp_addrs(mut self, tcp_addrs: Vec<A>) -> Self {
        self.tcp_addrs = tcp_addrs;
        self
    }

    /// Builds the [`PeerConnection`] and starts the background event loop driver.
    pub async fn build(self) -> Result<impl PeerConnection> {
        let runtime = if let Some(runtime) = self.runtime {
            runtime
        } else {
            default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?
        };

        let core = self.builder.build()?;

        PeerConnectionImpl::new(
            core,
            runtime,
            self.handler
                .ok_or_else(|| std::io::Error::other("no event handler found"))?,
            self.mdns_mode,
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
/// use webrtc::peer_connection::{RTCConfigurationBuilder, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct MyHandler;
/// #[async_trait::async_trait]
/// impl PeerConnectionEventHandler for MyHandler {}
///
/// # async fn example() -> webrtc::error::Result<()> {
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
    driver_handle: Mutex<Option<JoinHandle>>,
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
    /// RTP Transceivers
    pub(crate) rtp_transceivers: Mutex<HashMap<RTCRtpTransceiverId, Arc<RtpTransceiverImpl<I>>>>,
    /// Unified channel for all outgoing driver events
    pub(crate) driver_event_tx: Sender<PeerConnectionDriverEvent>,
    /// Coalescing write-flush gate (pion `awakeWriteLoop` equivalent).
    ///
    /// Hot-path senders (`dc.send`, etc.) set this flag and, only on the
    /// `false -> true` transition, drop a single non-blocking `WriteNotify` onto
    /// `driver_event_tx`. The driver clears the flag at the top of every loop
    /// iteration before draining core writes, so a burst of N sends produces at
    /// most one driver wake — replacing the old per-message
    /// `driver_event_tx.send(WriteNotify).await` (one blocking send per message).
    pub(crate) write_pending: AtomicBool,
    /// Counts coalesced sends (driver already behind) to drive a periodic
    /// cooperative yield — see [`PeerConnectionRef::wake_writes`].
    pub(crate) write_backpressure: std::sync::atomic::AtomicUsize,
    /// Channels for incoming data channel events
    pub(crate) data_channel_events_tx: Mutex<HashMap<RTCDataChannelId, Sender<DataChannelEvent>>>,
    /// Channels for incoming track remote events
    #[allow(clippy::type_complexity)]
    pub(crate) track_remote_events_tx:
        Mutex<HashMap<MediaStreamTrackId, (Sender<TrackRemoteEvent>, Arc<dyn TrackRemote>)>>,
}

/// Number of coalesced (driver-behind) sends between cooperative yields in
/// [`PeerConnectionRef::wake_writes`]. Roughly the batch the sender stuffs into
/// the SCTP buffer per driver wake; sized to amortise the wake without letting
/// the send buffer run far ahead of the ~1 MB SCTP window.
const WRITE_YIELD_INTERVAL: usize = 128;

impl<I> PeerConnectionRef<I>
where
    I: Interceptor,
{
    /// Coalescing driver wake for pending writes — the pion `awakeWriteLoop`
    /// equivalent. Marks a flush as pending and pokes the driver only on the
    /// `false -> true` transition, so a burst of sends yields at most one wake.
    ///
    /// The poke is a non-blocking `try_send`: if the channel is momentarily full
    /// a `WriteNotify` is already queued (or the driver is already draining), so
    /// dropping it is safe — the driver drains the core unconditionally each loop.
    ///
    /// When the flag is *already* set the driver has not caught up yet. We then
    /// cooperatively yield once every [`WRITE_YIELD_INTERVAL`] such sends. This
    /// mimics tokio's per-task poll budget (which the old per-message
    /// `send().await` leaned on implicitly): it lets the sender stuff a full
    /// batch into the SCTP buffer before handing the CPU to the driver, so the
    /// driver drains many packets per wake instead of ping-ponging one at a time.
    /// Without it a hot sender either starves the driver (no yield) or forces a
    /// 1:1 wake per message (yield every time) on cooperatively-scheduled
    /// runtimes such as smol — both collapse throughput.
    #[inline]
    pub(crate) async fn wake_writes(&self) {
        use std::sync::atomic::Ordering;
        if !self.write_pending.swap(true, Ordering::AcqRel) {
            let _ = self
                .driver_event_tx
                .try_send(PeerConnectionDriverEvent::WriteNotify);
        } else if self.write_backpressure.fetch_add(1, Ordering::Relaxed) % WRITE_YIELD_INTERVAL
            == WRITE_YIELD_INTERVAL - 1
        {
            crate::runtime::yield_now().await;
        }
    }
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
        mdns_mode: MulticastDnsMode,
        udp_addrs: Vec<A>,
        tcp_addrs: Vec<A>,
    ) -> Result<Self> {
        let async_mdns_socket = if mdns_mode != MulticastDnsMode::Disabled {
            let socket = MulticastSocket::new().into_std()?;
            Some(runtime.wrap_udp_socket(socket)?)
        } else {
            None
        };

        let mut async_udp_sockets = HashMap::new();
        for addr in udp_addrs {
            let socket = std::net::UdpSocket::bind(addr)?;
            socket.set_nonblocking(true)?;
            let local_addr = socket.local_addr()?;
            let async_udp_socket = runtime.wrap_udp_socket(socket)?;
            async_udp_sockets.insert(local_addr, async_udp_socket);
        }

        let mut async_tcp_listeners = HashMap::new();
        for addr in tcp_addrs {
            let listener = std::net::TcpListener::bind(addr)?;
            listener.set_nonblocking(true)?;
            let local_addr = listener.local_addr()?;
            let async_tcp_listener = runtime.wrap_tcp_listener(listener)?;
            async_tcp_listeners.insert(local_addr, async_tcp_listener);
        }

        let configuration = core.get_configuration();
        let ice_servers = configuration.ice_servers().to_vec();
        let ice_gather_policy = configuration.ice_transport_policy();

        let (driver_event_tx, driver_event_rx) =
            channel(PEER_CONNECTION_DRIVER_EVENT_CHANNEL_CAPACITY);
        let peer_connection = Self {
            inner: Arc::new(PeerConnectionRef {
                core: Mutex::new(core),
                runtime: runtime.clone(),
                data_channel_events_tx: Mutex::new(HashMap::new()),
                track_remote_events_tx: Mutex::new(HashMap::new()),
                rtp_transceivers: Mutex::new(HashMap::new()),
                handler,
                driver_event_tx,
                write_pending: AtomicBool::new(false),
                write_backpressure: std::sync::atomic::AtomicUsize::new(0),
            }),
            driver_handle: Mutex::new(None),
        };

        let local_addrs = async_udp_sockets.keys().cloned().collect::<Vec<_>>();
        let stun_gatherer =
            RTCStunGatherer::new(local_addrs.clone(), ice_servers.clone(), ice_gather_policy);
        let turn_relayer = RTCTurnRelayer::new(local_addrs, ice_servers, ice_gather_policy);

        let mut driver = PeerConnectionDriver::new(
            peer_connection.inner.clone(),
            stun_gatherer,
            turn_relayer,
            async_mdns_socket,
            async_udp_sockets,
            async_tcp_listeners,
        )
        .await?;
        let driver_handle = runtime.spawn(Box::pin(async move {
            if let Err(e) = driver.event_loop(driver_event_rx).await {
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
            .driver_event_tx
            .send(PeerConnectionDriverEvent::Close)
            .await
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
            .driver_event_tx
            .send(PeerConnectionDriverEvent::IceGathering)
            .await
            .map_err(|e| Error::Other(format!("{:?}", e)))
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
        // timer expired and never send the initial STUN binding requests. The
        // coalescing wake re-runs the whole loop (incl. poll_timeout), so this is
        // sufficient here just as the old WriteNotify was.
        self.inner.wake_writes().await;
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
        {
            let mut core = self.inner.core.lock().await;
            core.add_remote_candidate(candidate.clone())?;
        }

        let candidate_str = match candidate.candidate.strip_prefix("candidate:") {
            Some(s) => s,
            None => candidate.candidate.as_str(),
        };
        if let Ok(c) = rtc::ice::candidate::unmarshal_candidate(candidate_str)
            && c.network_type().is_tcp()
            && c.tcp_type() == rtc::ice::tcp_type::TcpType::Passive
        {
            self.inner
                .driver_event_tx
                .send(PeerConnectionDriverEvent::RemoteIceTcpPassiveCandidate(c))
                .await
                .map_err(|e| Error::Other(format!("{:?}", e)))
        } else {
            Ok(())
        }
    }

    async fn restart_ice(&self) -> Result<()> {
        {
            let mut core = self.inner.core.lock().await;
            core.restart_ice();
        }

        self.inner
            .driver_event_tx
            .send(PeerConnectionDriverEvent::IceGathering)
            .await
            .map_err(|e| Error::Other(format!("{:?}", e)))
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

        let (evt_tx, evt_rx) = channel(DATA_CHANNEL_EVENT_CHANNEL_CAPACITY);
        {
            let mut data_channels = self.inner.data_channel_events_tx.lock().await;
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
        let mut rtp_senders = vec![];
        for rtp_transceiver in self.get_transceivers().await {
            if let Ok(sender) = rtp_transceiver.sender().await
                && let Some(rtp_sender) = sender
            {
                rtp_senders.push(rtp_sender);
            }
        }
        rtp_senders
    }

    /// Get the list of rtp receiver
    async fn get_receivers(&self) -> Vec<Arc<dyn RtpReceiver>> {
        let mut rtp_receivers = vec![];
        for rtp_transceiver in self.get_transceivers().await {
            if let Ok(receiver) = rtp_transceiver.receiver().await
                && let Some(rtp_receiver) = receiver
            {
                rtp_receivers.push(rtp_receiver);
            }
        }
        rtp_receivers
    }

    /// Get the list of rtp transceiver
    async fn get_transceivers(&self) -> Vec<Arc<dyn RtpTransceiver>> {
        let current_transceiver_ids: HashSet<RTCRtpTransceiverId> = {
            let core = self.inner.core.lock().await;
            core.get_transceivers().collect::<HashSet<_>>()
        };

        let mut rtp_transceivers = self.inner.rtp_transceivers.lock().await;
        // only keep rtp_transceiver in current_transceiver_ids
        rtp_transceivers.retain(|id, _| current_transceiver_ids.contains(id));
        for id in current_transceiver_ids {
            rtp_transceivers
                .entry(id)
                .or_insert_with(|| Arc::new(RtpTransceiverImpl::new(id, Arc::clone(&self.inner))));
        }

        rtp_transceivers
            .values()
            .cloned()
            .map(|t| t as Arc<dyn RtpTransceiver>)
            .collect()
    }

    /// Add a Track to the PeerConnection
    async fn add_track(&self, track: Arc<dyn TrackLocal>) -> Result<Arc<dyn RtpSender>> {
        let id: RTCRtpTransceiverId = {
            let mut core = self.inner.core.lock().await;
            core.add_track(track.track().await)?.into()
        };

        let mut rtp_transceivers = self.inner.rtp_transceivers.lock().await;
        rtp_transceivers
            .entry(id)
            .or_insert_with(|| Arc::new(RtpTransceiverImpl::new(id, Arc::clone(&self.inner))));

        let rtp_transceiver = rtp_transceivers
            .get(&id)
            .ok_or(Error::ErrRTPTransceiverNotExisted)?;

        let sender: Arc<dyn RtpSender> = Arc::new(RtpSenderImpl::new(
            id.into(),
            Arc::clone(&self.inner),
            track,
        ));
        rtp_transceiver.set_sender(Some(Arc::clone(&sender))).await;

        Ok(sender)
    }

    /// Remove a Track from the PeerConnection
    async fn remove_track(&self, sender: &Arc<dyn RtpSender>) -> Result<()> {
        {
            let mut core = self.inner.core.lock().await;
            core.remove_track(sender.id())?;
        }

        let rtp_transceivers = self.inner.rtp_transceivers.lock().await;
        let rtp_transceiver = rtp_transceivers
            .get(&sender.id().into())
            .ok_or(Error::ErrRTPTransceiverNotExisted)?;
        rtp_transceiver.set_sender(None).await;

        Ok(())
    }

    /// Create a new RtpTransceiver(SendRecv or SendOnly) and add it to the set of transceivers
    async fn add_transceiver_from_track(
        &self,
        track: Arc<dyn TrackLocal>,
        init: Option<RTCRtpTransceiverInit>,
    ) -> Result<Arc<dyn RtpTransceiver>> {
        let id: RTCRtpTransceiverId = {
            let mut core = self.inner.core.lock().await;
            core.add_transceiver_from_track(track.track().await, init)?
        };

        let mut rtp_transceivers = self.inner.rtp_transceivers.lock().await;
        rtp_transceivers
            .entry(id)
            .or_insert_with(|| Arc::new(RtpTransceiverImpl::new(id, Arc::clone(&self.inner))));

        let rtp_transceiver = rtp_transceivers
            .get(&id)
            .ok_or(Error::ErrRTPTransceiverNotExisted)?;

        let sender: Arc<dyn RtpSender> = Arc::new(RtpSenderImpl::new(
            id.into(),
            Arc::clone(&self.inner),
            track,
        ));
        rtp_transceiver.set_sender(Some(sender)).await;

        Ok(rtp_transceiver.clone() as Arc<dyn RtpTransceiver>)
    }

    /// Create a new RtpTransceiver and adds it to the set of transceivers
    async fn add_transceiver_from_kind(
        &self,
        kind: RtpCodecKind,
        init: Option<RTCRtpTransceiverInit>,
    ) -> Result<Arc<dyn RtpTransceiver>> {
        let (id, track) = {
            let mut core = self.inner.core.lock().await;
            let id = core.add_transceiver_from_kind(kind, init)?;
            (
                id,
                core.rtp_sender(id.into())
                    .map(|sender| sender.track().clone()),
            )
        };

        let mut rtp_transceivers = self.inner.rtp_transceivers.lock().await;
        rtp_transceivers
            .entry(id)
            .or_insert_with(|| Arc::new(RtpTransceiverImpl::new(id, Arc::clone(&self.inner))));

        let rtp_transceiver = rtp_transceivers
            .get(&id)
            .ok_or(Error::ErrRTPTransceiverNotExisted)?;

        if let Some(track) = track {
            let sender: Arc<dyn RtpSender> = Arc::new(RtpSenderImpl::new(
                id.into(),
                Arc::clone(&self.inner),
                Arc::new(TrackLocalStaticRTP::new(track)),
            ));
            rtp_transceiver.set_sender(Some(sender)).await;
        }

        Ok(rtp_transceiver.clone() as Arc<dyn RtpTransceiver>)
    }

    /// Get a snapshot of accumulated statistics.
    async fn get_stats(&self, now: Instant, selector: StatsSelector) -> RTCStatsReport {
        let mut core = self.inner.core.lock().await;
        core.get_stats(now, selector)
    }
}
