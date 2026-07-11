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
use std::sync::atomic::{AtomicBool, Ordering};

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

use crate::media_stream::track_local::TrackLocalEvent;
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
    dedicated_reactor: bool,
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
            dedicated_reactor: false,
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
            dedicated_reactor: self.dedicated_reactor,
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

    /// Run this peer connection's driver on its own dedicated OS thread with a
    /// single-threaded reactor, instead of on the shared async runtime.
    ///
    /// This *confines* the driver (and thus its SCTP/DTLS/SRTP state and I/O
    /// reactor) to a single dedicated thread, so the async runtime never migrates
    /// it across its worker pool — the dominant cost for in-process data-channel
    /// throughput on multi-threaded runtimes (issue #101). It costs one OS thread
    /// per peer connection, so it is **off by default**: enable it for
    /// latency/throughput-sensitive deployments with a modest number of
    /// connections, and leave it off for large-scale servers (e.g. SFUs) with
    /// thousands of connections.
    ///
    /// Note: this is *thread confinement*, not CPU-core affinity — the OS
    /// scheduler may still move the dedicated thread between cores.
    /// TODO(#101): pin the reactor thread to a specific core (via `core_affinity`)
    /// for cache/NUMA locality as a follow-up.
    ///
    /// Note: with this enabled, event-handler callbacks run on the dedicated
    /// reactor thread, so they must not block.
    pub fn with_dedicated_reactor_thread(mut self, enabled: bool) -> Self {
        self.dedicated_reactor = enabled;
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
            self.dedicated_reactor,
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
    /// Whether the driver runs on a dedicated reactor thread. When true, `close()`
    /// waits for that thread to finish, and `Drop` signals it to stop (via
    /// [`PeerConnectionRef::closing`]) so it does not leak if the connection is
    /// dropped without an explicit `close()`.
    dedicated_reactor: bool,
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
    /// Shutdown flag set by `close()`/`Drop`. The driver checks it at the top of
    /// every loop iteration, so the event loop — and thus a dedicated reactor
    /// thread — terminates even when the accompanying best-effort `Close` wake
    /// could not be enqueued (a momentarily full channel). This is the guarantee
    /// that closes the reactor-thread leak window; the `Close` event is only the
    /// fast wake.
    pub(crate) closing: AtomicBool,
    /// Channels for incoming data channel events
    pub(crate) data_channel_events_tx: Mutex<HashMap<RTCDataChannelId, Sender<DataChannelEvent>>>,
    /// Channels for incoming track remote events
    #[allow(clippy::type_complexity)]
    pub(crate) track_remote_events_tx:
        Mutex<HashMap<MediaStreamTrackId, (Sender<TrackRemoteEvent>, Arc<dyn TrackRemote>)>>,
    /// Channels for delivering RTCP feedback to local (sent) tracks, keyed by track id.
    pub(crate) track_local_events_tx: Mutex<HashMap<MediaStreamTrackId, Sender<TrackLocalEvent>>>,
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
        dedicated_reactor: bool,
    ) -> Result<Self> {
        // Bind the std sockets up front (synchronous, and needed to compute the
        // local addresses used for ICE gathering / SDP). Wrapping them into async
        // I/O resources is deferred so it can happen on whichever runtime actually
        // drives the event loop: with a dedicated reactor thread, tokio I/O
        // resources must be created on the reactor that polls them, so wrapping is
        // done inside the reactor future (see `run_driver`) rather than here.
        let std_mdns_socket = if mdns_mode != MulticastDnsMode::Disabled {
            Some(MulticastSocket::new().into_std()?)
        } else {
            None
        };

        let mut std_udp_sockets = Vec::new();
        for addr in udp_addrs {
            let socket = std::net::UdpSocket::bind(addr)?;
            socket.set_nonblocking(true)?;
            let local_addr = socket.local_addr()?;
            std_udp_sockets.push((local_addr, socket));
        }

        let mut std_tcp_listeners = Vec::new();
        for addr in tcp_addrs {
            let listener = std::net::TcpListener::bind(addr)?;
            listener.set_nonblocking(true)?;
            let local_addr = listener.local_addr()?;
            std_tcp_listeners.push((local_addr, listener));
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
                track_local_events_tx: Mutex::new(HashMap::new()),
                rtp_transceivers: Mutex::new(HashMap::new()),
                handler,
                driver_event_tx,
                write_pending: AtomicBool::new(false),
                write_backpressure: std::sync::atomic::AtomicUsize::new(0),
                closing: AtomicBool::new(false),
            }),
            driver_handle: Mutex::new(None),
            dedicated_reactor,
        };

        let local_addrs = std_udp_sockets
            .iter()
            .map(|(addr, _)| *addr)
            .collect::<Vec<_>>();
        let stun_gatherer =
            RTCStunGatherer::new(local_addrs.clone(), ice_servers.clone(), ice_gather_policy);
        let turn_relayer = RTCTurnRelayer::new(local_addrs, ice_servers, ice_gather_policy);

        // Init-result oneshot. `new()` awaits this so that socket wrapping and
        // driver construction errors propagate out of `build()`, instead of being
        // silently logged on the driver thread — which would otherwise leave a
        // healthy-looking `PeerConnection` in front of a dead driver (e.g. a
        // `wrap_udp_socket` failure under an exhausted fd limit). Init is fast
        // (socket wrapping + driver construction); the event loop then runs
        // fire-and-forget.
        let (init_tx, mut init_rx) = channel::<Result<()>>(1);

        // The reactor body: wrap the bound sockets on the runtime that runs this
        // future, build the driver, report the init outcome, then run the event
        // loop to completion.
        let inner = peer_connection.inner.clone();
        let driver_runtime = runtime.clone();
        let run_driver = async move {
            let init: Result<PeerConnectionDriver<I>> = async {
                let async_mdns_socket = match std_mdns_socket {
                    Some(socket) => Some(driver_runtime.wrap_udp_socket(socket)?),
                    None => None,
                };
                let mut async_udp_sockets = HashMap::new();
                for (local_addr, socket) in std_udp_sockets {
                    async_udp_sockets.insert(local_addr, driver_runtime.wrap_udp_socket(socket)?);
                }
                let mut async_tcp_listeners = HashMap::new();
                for (local_addr, listener) in std_tcp_listeners {
                    async_tcp_listeners
                        .insert(local_addr, driver_runtime.wrap_tcp_listener(listener)?);
                }

                PeerConnectionDriver::new(
                    inner,
                    stun_gatherer,
                    turn_relayer,
                    async_mdns_socket,
                    async_udp_sockets,
                    async_tcp_listeners,
                )
                .await
            }
            .await;

            let mut driver = match init {
                Ok(driver) => {
                    // Capacity-1 channel, sent exactly once → `try_send` never Full.
                    let _ = init_tx.try_send(Ok(()));
                    driver
                }
                Err(e) => {
                    let _ = init_tx.try_send(Err(e));
                    return;
                }
            };

            if let Err(e) = driver.event_loop(driver_event_rx).await {
                error!("I/O error: {}", e);
            }
        };

        let driver_handle = if dedicated_reactor {
            runtime.spawn_reactor(Box::pin(run_driver))
        } else {
            runtime.spawn(Box::pin(run_driver))
        };
        *peer_connection.driver_handle.lock().await = Some(driver_handle);

        // Surface init errors here rather than swallowing them on the driver
        // thread. The driver reports its init outcome exactly once; a closed
        // channel means the driver future was dropped before initialising.
        match init_rx.recv().await {
            Some(Ok(())) => Ok(peer_connection),
            Some(Err(e)) => Err(e),
            None => Err(Error::Other(
                "peer connection driver stopped before initialization".to_owned(),
            )),
        }
    }
}

impl<I> Drop for PeerConnectionImpl<I>
where
    I: Interceptor,
{
    fn drop(&mut self) {
        // A dedicated reactor thread only exits when its event loop returns, so
        // a connection dropped without an explicit `close()` would leak the
        // thread. Set the shutdown flag (infallible) so the driver stops at the
        // top of its next loop iteration, then best-effort wake it so it stops
        // promptly rather than after its next timer/socket event. Crucially the
        // flag — not the wake — is the guarantee: a full channel drops the wake
        // but cannot leak the thread. (Task-based drivers detach harmlessly, so
        // this is limited to the dedicated-reactor case to avoid changing the
        // default lifecycle.)
        if self.dedicated_reactor {
            self.inner.closing.store(true, Ordering::Release);
            let _ = self
                .inner
                .driver_event_tx
                .try_send(PeerConnectionDriverEvent::Close);
        }
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
        // Mark closing before waking the driver, so it stops even if the wake is
        // ever dropped (mirrors `Drop`; see `PeerConnectionRef::closing`).
        self.inner.closing.store(true, Ordering::Release);
        // Best-effort wake. A send failure here is benign, not an error:
        // `closing` already guarantees the driver terminates, and it may already
        // have observed the flag and dropped the receiver via its independent
        // top-of-loop exit path — in which case the channel is closed. Treating
        // that as an error would make a perfectly clean shutdown return `Err`.
        let _ = self
            .inner
            .driver_event_tx
            .send(PeerConnectionDriverEvent::Close)
            .await;

        let driver_handle = self.driver_handle.lock().await.take();
        if let Some(driver_handle) = driver_handle {
            if self.dedicated_reactor {
                // The reactor runs on its own OS thread that cannot be aborted;
                // wait (bounded) for its event loop to actually return, so the
                // socket and thread are released by the time `close()` resolves.
                // It exits promptly once it observes the shutdown above; the bound
                // only prevents `close()` from hanging on a wedged reactor, after
                // which the handle is dropped (detached) as a last resort.
                //
                // Note: if `close()` is called *from within an event-handler
                // callback* (which, for a dedicated reactor, runs on this very
                // thread), the loop cannot make progress until the handler
                // returns, so this wait runs out its full bound before detaching.
                // Handlers must not block (see `with_dedicated_reactor_thread`).
                let step = std::time::Duration::from_millis(1);
                let max = std::time::Duration::from_secs(2);
                let mut waited = std::time::Duration::ZERO;
                while !driver_handle.is_finished() && waited < max {
                    crate::runtime::sleep(step).await;
                    waited += step;
                }
            } else {
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
