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
pub use rtc::statistics::StatsSelector;
pub use rtc::statistics::report::{RTCStatsReport, RTCStatsReportEntry};

use crate::media_stream::track_local::TrackLocalEvent;
use crate::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use crate::media_stream::track_remote::TrackRemoteEvent;
use crate::peer_connection::driver::PeerConnectionDriverEvent;
use crate::rtp_transceiver::rtp_sender::RtpSenderImpl;
pub use rtc::interceptor::{Interceptor, NoopInterceptor, Registry};

// Argument types for `SettingEngine`'s DTLS/SRTP setters. Re-exported because `rtc` is a
// private dependency of this crate: without these, calling `set_dtls_cipher_suites` or
// `set_srtp_protection_profiles` would force an application to add a second, version-locked
// dependency just to name the enum it passes in.
/// The crypto provider API, re-exported for the same reason: `SettingEngine::set_crypto_provider`
/// takes an `Arc<dyn RTCCryptoProvider>`, and an application implementing its own provider needs
/// the traits too.
pub use rtc::crypto;
pub use rtc::dtls::cipher_suite::CipherSuiteId;
pub use rtc::dtls::extension::extension_use_srtp::SrtpProtectionProfile;
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
    udp_addrs: Vec<A>,
    tcp_addrs: Vec<A>,
    dedicated_reactor_pool_size: usize,
    data_channel_send_buffer_limit: usize,
    /// Held rather than forwarded immediately, so [`build`](Self::build) can resolve the crypto
    /// provider and inject it before the core connection is constructed — see there for why.
    setting_engine: SettingEngine,
}

impl<A: ToSocketAddrs> Default for PeerConnectionBuilder<A, NoopInterceptor> {
    fn default() -> Self {
        Self {
            builder: RTCPeerConnectionBuilder::new(),
            runtime: None,
            handler: None,
            udp_addrs: vec![],
            tcp_addrs: vec![],
            dedicated_reactor_pool_size: 0,
            setting_engine: SettingEngine::default(),
            // `usize::MAX` = unbounded: no send back-pressure unless the application
            // opts in via `with_data_channel_send_buffer_limit`. This keeps `send`/
            // `send_text` non-blocking by default (zero behaviour change).
            data_channel_send_buffer_limit: usize::MAX,
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
    I: Interceptor + 'static,
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
        self.setting_engine = setting_engine;
        self
    }

    /// Configures the builder with the specified interceptor [`Registry`].
    ///
    /// The chain's type parameter stays on the *builder* and never escapes [`Self::build`],
    /// which hands back an opaque `impl PeerConnection` — so callers do not need
    /// `rtc`'s [`Registry::boxed`](rtc::interceptor::Registry::boxed) to keep the interceptor
    /// type out of their own structs. Pass the registry as-is.
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
            dedicated_reactor_pool_size: self.dedicated_reactor_pool_size,
            data_channel_send_buffer_limit: self.data_channel_send_buffer_limit,
            setting_engine: self.setting_engine,
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

    /// Set the size of the dedicated reactor pool used. Defaults to `0`, means disabled.
    /// Values above `1024` are clamped down to it.
    ///
    /// The value is handed to
    /// [`Runtime::spawn_reactor`] when this
    /// connection's driver is spawned, but each built-in runtime builds its pool **once**,
    /// lazily, on first use. Only the first dedicated-reactor connection's value therefore
    /// takes effect for the process — set it consistently across connections, or set it on
    /// whichever you build first.
    ///
    /// Setting this to `0` disables the dedicated reactor pool; any non-zero value enables it.
    ///
    /// Smaller pools use fewer threads and less memory (fewer per-thread allocator
    /// arenas) at the cost of more drivers sharing each thread; size it to trade
    /// resident memory against per-connection isolation for your workload.
    pub fn with_dedicated_reactor_pool_size(mut self, dedicated_reactor_pool_size: usize) -> Self {
        self.dedicated_reactor_pool_size = dedicated_reactor_pool_size;
        self
    }

    /// Sets the per-channel data-channel send-buffer limit, in bytes, opting into send
    /// back-pressure.
    ///
    /// Once set, a channel's outstanding send bytes (handed to `send`/`send_text` but
    /// not yet acknowledged or abandoned by SCTP) are bounded by this limit:
    ///
    /// - [`DataChannel::send`] / [`DataChannel::send_text`] **block** until the buffer
    ///   is below the limit, then enqueue — mirroring `tokio::mpsc::Sender::send`.
    /// - [`DataChannel::try_send`] / [`DataChannel::try_send_text`] instead **fail fast**
    ///   with [`Error::ErrSendBufferFull`] — mirroring `tokio::mpsc::Sender::try_send`.
    /// - [`DataChannel::writable`] resolves once the buffer is below the limit.
    ///
    /// The limit is applied to **each data channel independently**, so a connection with
    /// `N` channels can hold up to `N × limit` outstanding across all of them — size it
    /// for a per-channel budget, not a whole-connection cap.
    ///
    /// **Default: `usize::MAX` (unbounded)** — no back-pressure, and `send`/`send_text`
    /// never block, matching the historical behaviour and Safari/Firefox (which impose no
    /// send-queue cap). Passing `0` is also treated as unbounded. As a reference point,
    /// Chromium caps its `RTCDataChannel` send queue at 16 MiB
    /// (`webrtc::DataChannelInterface::MaxSendQueueSize`); `16 * 1024 * 1024` is a
    /// reasonable browser-like value, well above the ~1 MiB SCTP receive window so a
    /// sender pacing on `OnBufferedAmountLow` never hits it.
    pub fn with_data_channel_send_buffer_limit(mut self, bytes: usize) -> Self {
        self.data_channel_send_buffer_limit = bytes;
        self
    }

    /// Builds the [`PeerConnection`] and starts the background event loop driver.
    pub async fn build(mut self) -> Result<impl PeerConnection> {
        let runtime = if let Some(runtime) = self.runtime {
            runtime
        } else {
            default_runtime().ok_or_else(|| std::io::Error::other("no async runtime found"))?
        };

        // Resolve the crypto provider here, once, and hand the *same* `Arc` to both the core
        // connection and the async layer's TURN client. `crypto::default_provider()` allocates a
        // fresh provider per call, so resolving independently on each side would leave the
        // connection using two — hence: read what the caller configured, fall back to the
        // built-in, then inject it so `rtc` construction adopts it rather than resolving again.
        let crypto_provider = match self.setting_engine.crypto_provider() {
            Some(provider) => provider.clone(),
            None => crypto::default_provider().map_err(|error| {
                Error::Crypto(format!(
                    "failed to resolve a default crypto provider: {error}"
                ))
            })?,
        };
        self.setting_engine
            .set_crypto_provider(crypto_provider.clone());
        let mdns_mode = self.setting_engine.multicast_dns().mode;

        // The core is told the time from here on; this is the seed every construction-time
        // instant inside it derives from, and under a `MockRuntime` it is the virtual clock's.
        let core = self
            .builder
            .with_setting_engine(self.setting_engine)
            .build(runtime.now())?;

        // `0` = unbounded (same as the `usize::MAX` default); normalise it to `usize::MAX`
        // so the send-buffer gate (and `writable()`) short-circuits to a no-op.
        let data_channel_send_buffer_limit = if self.data_channel_send_buffer_limit == 0 {
            usize::MAX
        } else {
            self.data_channel_send_buffer_limit
        };

        PeerConnectionImpl::new(
            core,
            runtime,
            self.handler
                .ok_or_else(|| std::io::Error::other("no event handler found"))?,
            mdns_mode,
            self.udp_addrs,
            self.tcp_addrs,
            self.dedicated_reactor_pool_size,
            data_channel_send_buffer_limit,
            crypto_provider,
        )
        .await
    }
}

/// Object-safe trait exposing all public PeerConnection operations.
///
/// [`PeerConnectionBuilder::build`] returns an opaque `impl PeerConnection`, hiding the
/// generic interceptor type. Because this trait is object safe, wrap that value in
/// `Arc<dyn PeerConnection>` when you need to store the connection in your own type or
/// share it across tasks:
///
/// ```ignore
/// let pc: Arc<dyn PeerConnection> = Arc::new(builder.build().await?);
/// ```
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
pub trait PeerConnection: crate::sealed::Sealed + Send + Sync + 'static {
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
/// Not exposed directly — obtained as an opaque `impl PeerConnection` from
/// [`PeerConnectionBuilder::build`].
pub(crate) struct PeerConnectionImpl<I = NoopInterceptor>
where
    I: Interceptor,
{
    inner: Arc<PeerConnectionRef<I>>,
    driver_handle: Mutex<Option<Box<dyn JoinHandle>>>,
    /// Whether the driver runs on the shared bounded reactor pool (a task pinned to
    /// one pool thread) rather than the general async runtime. When true, `close()`
    /// waits for that task to finish and then aborts it, and `Drop` signals it to
    /// stop (via [`PeerConnectionRef::closing`]) so a driver task is not left
    /// running on a pool thread if the connection is dropped without an explicit
    /// `close()`.
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
    /// Per-channel data-channel send-buffer limit in bytes (`usize::MAX` = unbounded,
    /// the default). When a limit is configured, [`DataChannel::send`]/[`send_text`](DataChannel::send_text)
    /// block until a channel's `outstanding_bytes` drops below it, and
    /// [`DataChannel::try_send`]/[`try_send_text`](DataChannel::try_send_text) fail fast with
    /// `ErrSendBufferFull` — an opt-in bound on send-side memory. Set once at build time via
    /// [`PeerConnectionBuilder::with_data_channel_send_buffer_limit`].
    pub(crate) data_channel_send_buffer_limit: usize,
    /// Woken by the driver once per event-loop iteration after it applies SCTP buffer
    /// releases (acknowledged/abandoned bytes) to each channel's `outstanding_bytes`,
    /// and by `close()`/`Drop`. A [`DataChannel::writable`] future blocked on the
    /// send-buffer limit waits on this and re-checks, so it unblocks as soon as the
    /// peer acknowledges data (or the connection closes). Dormant unless a
    /// `data_channel_send_buffer_limit` is configured — a `usize::MAX` (default) limit
    /// never parks on it.
    pub(crate) data_channel_backpressure: crate::runtime::Notify,
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
            self.runtime.yield_now().await;
        }
    }
}

impl<I> PeerConnectionImpl<I>
where
    I: Interceptor + 'static,
{
    /// Create a new peer connection with a custom runtime
    #[allow(clippy::too_many_arguments)] // private constructor fanned out from the builder
    async fn new<A: ToSocketAddrs>(
        core: RTCPeerConnection<I>,
        runtime: Arc<dyn Runtime>,
        handler: Arc<dyn PeerConnectionEventHandler>,
        mdns_mode: MulticastDnsMode,
        udp_addrs: Vec<A>,
        tcp_addrs: Vec<A>,
        dedicated_reactor_pool_size: usize,
        data_channel_send_buffer_limit: usize,
        crypto_provider: Arc<dyn crypto::RTCCryptoProvider>,
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
                data_channel_send_buffer_limit,
                data_channel_backpressure: crate::runtime::Notify::new(),
            }),
            driver_handle: Mutex::new(None),
            dedicated_reactor: dedicated_reactor_pool_size > 0,
        };

        let local_addrs = std_udp_sockets
            .iter()
            .map(|(addr, _)| *addr)
            .collect::<Vec<_>>();
        let stun_gatherer = RTCStunGatherer::new(
            local_addrs.clone(),
            ice_servers.clone(),
            ice_gather_policy,
            Arc::clone(&runtime),
        );
        let turn_relayer = RTCTurnRelayer::new(
            local_addrs,
            ice_servers,
            ice_gather_policy,
            Arc::clone(&runtime),
            crypto_provider,
        );

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
            // The driver has stopped for good (clean shutdown OR an abnormal error exit).
            // Mark closing and wake any sender parked in send back-pressure, so a blocking
            // send() cannot hang waiting for a drain that will never come — the driver no
            // longer drains outstanding_bytes. Idempotent when close()/Drop already set it.
            driver.signal_stopped();
        };

        let driver_handle = if dedicated_reactor_pool_size > 0 {
            runtime.spawn_reactor(dedicated_reactor_pool_size, Box::pin(run_driver))
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
        // A reactor-pool driver task only exits when its event loop returns, so a
        // connection dropped without an explicit `close()` would leave that task
        // running on a shared pool thread — pinning a scarce reactor thread and
        // holding the connection's buffers alive (the RSS this pool exists to
        // bound). Dropping the join handle merely detaches the task. So set the
        // shutdown flag (infallible) so the driver stops at the top of its next
        // loop iteration, then best-effort wake it so it stops promptly rather
        // than after its next timer/socket event. Crucially the flag — not the
        // wake — is the guarantee: a full channel drops the wake but cannot leak
        // the task. (Drivers on the general runtime detach harmlessly onto the
        // application's own worker pool, so this is limited to the pooled-reactor
        // case to avoid changing the default lifecycle.)
        if self.dedicated_reactor {
            self.inner.closing.store(true, Ordering::Release);
            // Wake a sender blocked in `DataChannel::writable()` so it returns promptly
            // instead of waiting out its 50 ms backstop past teardown (mirrors `close`).
            self.inner.data_channel_backpressure.notify_waiters();
            let _ = self
                .inner
                .driver_event_tx
                .try_send(PeerConnectionDriverEvent::Close);
        }
    }
}

impl<I> crate::sealed::Sealed for PeerConnectionImpl<I> where I: Interceptor + 'static {}

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
        // Wake any sender blocked in `DataChannel::writable()` so it observes `closing`
        // and returns `ErrDataChannelClosed` at once, rather than waiting out its 50 ms
        // liveness backstop — the driver has stopped draining `outstanding_bytes`.
        self.inner.data_channel_backpressure.notify_waiters();
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
                // The reactor driver is a task pinned to a shared pool thread.
                // First wait (bounded) for its event loop to return on its own, so
                // it flushes the SCTP shutdown and releases its socket by the time
                // `close()` resolves — it exits promptly once it observes the
                // shutdown signalled above. Then abort the task unconditionally to
                // free the pool thread of it (a no-op once it has finished; the
                // fallback that reclaims a driver still wedged at the bound).
                //
                // Note: if `close()` is called *from within an event-handler
                // callback* (which, for a dedicated reactor, runs on this very
                // task), the loop cannot make progress until the handler returns,
                // so this wait runs out its full bound before aborting. Handlers
                // must not block (see `with_dedicated_reactor_thread`).
                let step = std::time::Duration::from_millis(1);
                let max = std::time::Duration::from_secs(2);
                let mut waited = std::time::Duration::ZERO;
                while !driver_handle.is_finished() && waited < max {
                    self.inner.runtime.sleep(step).await;
                    waited += step;
                }
                driver_handle.abort();
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
            core.set_local_description(self.inner.runtime.now(), desc)?;
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
            core.set_remote_description(self.inner.runtime.now(), desc)?;
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
        let (ice_servers, ice_transport_policy) = {
            let mut core = self.inner.core.lock().await;
            core.set_configuration(configuration)?;
            let configuration = core.get_configuration();
            (
                configuration.ice_servers().to_vec(),
                configuration.ice_transport_policy(),
            )
        };

        self.inner
            .driver_event_tx
            .send(PeerConnectionDriverEvent::UpdateIceConfiguration {
                ice_servers,
                ice_transport_policy,
            })
            .await
            .map_err(|_| Error::Other("peer connection driver stopped".to_owned()))
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

        self.inner.wake_writes().await;

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

#[cfg(all(test, any(feature = "crypto-ring", feature = "crypto-aws-lc-rs")))]
pub(crate) use tests::new_test_peer_connection;

// A built-in provider is required: these construct real peer connections, and construction
// resolves a provider. The no-built-in configuration is exercised by the provider tests in
// `tests/`, which supply their own.
#[cfg(all(test, any(feature = "crypto-ring", feature = "crypto-aws-lc-rs")))]
mod tests {
    use super::*;
    use crate::runtime::{channel, default_runtime, timeout};
    use rtc::peer_connection::RTCPeerConnectionBuilder;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use std::time::Duration;

    #[derive(Clone)]
    struct DummyHandler;

    #[async_trait::async_trait]
    impl PeerConnectionEventHandler for DummyHandler {}

    pub(crate) async fn new_test_peer_connection() -> (
        Arc<PeerConnectionRef>,
        crate::runtime::Receiver<PeerConnectionDriverEvent>,
    ) {
        let core = RTCPeerConnectionBuilder::new()
            .build(Instant::now())
            .unwrap();
        let runtime = default_runtime().expect("test requires a runtime feature");
        let handler: Arc<dyn PeerConnectionEventHandler> = Arc::new(DummyHandler);
        let (driver_event_tx, driver_event_rx) = channel::<PeerConnectionDriverEvent>(1);

        let inner = Arc::new(PeerConnectionRef {
            core: Mutex::new(core),
            runtime,
            handler,
            driver_event_tx,
            write_pending: AtomicBool::new(false),
            write_backpressure: AtomicUsize::new(0),
            closing: AtomicBool::new(false),
            data_channel_send_buffer_limit: usize::MAX,
            data_channel_backpressure: crate::runtime::Notify::new(),
            data_channel_events_tx: Mutex::new(HashMap::new()),
            track_remote_events_tx: Mutex::new(HashMap::new()),
            track_local_events_tx: Mutex::new(HashMap::new()),
            rtp_transceivers: Mutex::new(HashMap::new()),
        });

        (inner, driver_event_rx)
    }

    #[test]
    fn create_data_channel_wakes_driver() {
        // Drive on the runtime under test rather than a bare executor: `timeout` below arms
        // a real timer, which needs that runtime's reactor.
        let rt = default_runtime().expect("test requires a runtime feature");
        rt.block_on(Box::pin(async {
            let (inner, mut driver_event_rx) = new_test_peer_connection().await;

            let pc = PeerConnectionImpl {
                inner,
                driver_handle: Mutex::new(None),
                dedicated_reactor: false,
            };

            let _dc = pc.create_data_channel("test", None).await.unwrap();

            let event = timeout(&*rt, Duration::from_secs(1), driver_event_rx.recv())
                .await
                .expect("driver should be woken within 1s")
                .expect("driver event channel should not be closed");
            assert!(matches!(event, PeerConnectionDriverEvent::WriteNotify));
        }));
    }
}

/// End-to-end tests that the sans-I/O core's timing-dependent behaviour is driven by the
/// runtime's clock rather than the wall clock.
///
/// These are the acceptance criteria for the deterministic-time work: the pluggable-runtime
/// post claims `MockRuntime` makes ICE timeouts, DTLS retransmits and SCTP RTO testable
/// instantly, and until the driver read `Runtime::now()` that was not true.
#[cfg(all(test, feature = "runtime-mock"))]
mod virtual_clock_tests {
    use super::*;
    use crate::runtime::mock::{MockRuntime, MockUDPNetwork};
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    /// Records the ICE connection states a peer reports, so a test can assert a transition
    /// without polling internal state.
    #[derive(Debug, Default)]
    struct StateRecorder {
        ice: StdMutex<Vec<RTCIceConnectionState>>,
        /// Locally gathered candidates not yet handed to the peer.
        pending_candidates: StdMutex<Vec<RTCIceCandidateInit>>,
        conn: StdMutex<Vec<RTCPeerConnectionState>>,
    }

    impl StateRecorder {
        fn ice_states(&self) -> Vec<RTCIceConnectionState> {
            self.ice.lock().expect("recorder poisoned").clone()
        }

        fn saw_ice(&self, state: RTCIceConnectionState) -> bool {
            self.ice_states().contains(&state)
        }

        fn saw_conn(&self, state: RTCPeerConnectionState) -> bool {
            self.conn
                .lock()
                .expect("recorder poisoned")
                .contains(&state)
        }

        fn conn_states(&self) -> Vec<RTCPeerConnectionState> {
            self.conn.lock().expect("recorder poisoned").clone()
        }

        fn take_candidates(&self) -> Vec<RTCIceCandidateInit> {
            std::mem::take(&mut *self.pending_candidates.lock().expect("recorder poisoned"))
        }
    }

    #[async_trait::async_trait]
    impl PeerConnectionEventHandler for StateRecorder {
        async fn on_ice_connection_state_change(&self, state: RTCIceConnectionState) {
            self.ice.lock().expect("recorder poisoned").push(state);
        }

        async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
            self.conn.lock().expect("recorder poisoned").push(state);
        }

        async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
            if let Ok(init) = event.candidate.to_json() {
                self.pending_candidates
                    .lock()
                    .expect("recorder poisoned")
                    .push(init);
            }
        }
    }

    /// Let the driver threads run. `MockRuntime::spawn` gives each task its own OS thread, so
    /// the test thread has to yield for them to make progress — but it yields, it does not
    /// wait on a protocol deadline. Every *protocol* timeout is reached by advancing the
    /// virtual clock, never by sleeping.
    fn settle() {
        std::thread::yield_now();
        std::thread::sleep(Duration::from_millis(2));
    }

    /// Advance both peers' clocks together and let their drivers run.
    fn advance_both(a: &MockRuntime, b: &MockRuntime, delta: Duration) {
        a.clock().advance(delta);
        b.clock().advance(delta);
        settle();
    }

    struct Peer {
        pc: Box<dyn PeerConnection>,
        rt: Arc<MockRuntime>,
        rec: Arc<StateRecorder>,
    }

    /// How far the virtual clock is pushed ahead of the wall clock before anything is built.
    ///
    /// This is what makes these tests *falsifiable*. Both clocks start at roughly the same
    /// instant, so a driver reading the wall clock behaves almost identically to one reading a
    /// virtual clock that has barely moved — the bug hides. Offsetting the virtual clock by an
    /// hour first means a wall-clock read is an hour in the past relative to every deadline the
    /// core computes, and nothing works at all.
    const CLOCK_OFFSET: Duration = Duration::from_secs(3600);

    async fn build_peer(network: &Arc<MockUDPNetwork>) -> Peer {
        let rt = Arc::new(MockRuntime::with_network(Arc::clone(network)));
        rt.clock().advance(CLOCK_OFFSET);
        let rec = Arc::new(StateRecorder::default());
        // mDNS binds the fixed port 5353, which two peers on one mock network would collide
        // on (a real stack shares it via SO_REUSEADDR). Resolving `.local` candidates is not
        // what these tests are about, so turn it off rather than model port sharing.
        let mut setting_engine = SettingEngine::default();
        setting_engine.set_multicast_dns_mode(rtc::ice::mdns::MulticastDnsMode::Disabled);
        let pc = PeerConnectionBuilder::new()
            .with_setting_engine(setting_engine)
            .with_runtime(Arc::clone(&rt) as Arc<dyn Runtime>)
            .with_handler(Arc::clone(&rec) as Arc<dyn PeerConnectionEventHandler>)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .expect("a peer connection builds on the mock runtime");
        Peer {
            pc: Box::new(pc),
            rt,
            rec,
        }
    }

    /// Two peers, ICE-connected, entirely on the mock network under virtual clocks.
    ///
    /// Returns once both report `Connected`, or panics with what they did report.
    async fn connect_pair(network: &Arc<MockUDPNetwork>) -> (Peer, Peer) {
        let offerer = build_peer(network).await;
        let answerer = build_peer(network).await;

        offerer
            .pc
            .create_data_channel("probe", None)
            .await
            .expect("create data channel");

        let offer = offerer.pc.create_offer(None).await.expect("create offer");
        offerer
            .pc
            .set_local_description(offer.clone())
            .await
            .expect("set local description");
        answerer
            .pc
            .set_remote_description(offer)
            .await
            .expect("set remote description");
        let answer = answerer
            .pc
            .create_answer(None)
            .await
            .expect("create answer");
        answerer
            .pc
            .set_local_description(answer.clone())
            .await
            .expect("set local description");
        offerer
            .pc
            .set_remote_description(answer)
            .await
            .expect("set remote description");

        // Drive the connectivity checks by advancing time, not by waiting for it, trickling
        // each peer's gathered candidates to the other as they appear.
        for _ in 0..200 {
            for c in offerer.rec.take_candidates() {
                answerer.pc.add_ice_candidate(c).await.ok();
            }
            for c in answerer.rec.take_candidates() {
                offerer.pc.add_ice_candidate(c).await.ok();
            }
            if offerer.rec.saw_ice(RTCIceConnectionState::Connected)
                && answerer.rec.saw_ice(RTCIceConnectionState::Connected)
            {
                return (offerer, answerer);
            }
            advance_both(&offerer.rt, &answerer.rt, Duration::from_millis(50));
        }

        panic!(
            "ICE did not connect under the virtual clock; offerer saw {:?}, answerer saw {:?}",
            offerer.rec.ice_states(),
            answerer.rec.ice_states()
        );
    }

    /// DTLS and SCTP are the other two behaviours the pluggable-runtime post names. Both sit
    /// *behind* ICE: the handshake only starts once a candidate pair is selected, and SCTP's
    /// association only once DTLS completes. So a data channel opening at all is the
    /// end-to-end proof that both of their timers ran on the virtual clock — the DTLS
    /// handshake's retransmit timer and SCTP's INIT/RTO are the only things that can drive
    /// them, and no wall-clock time is available for either.
    #[test]
    fn dtls_and_sctp_complete_under_a_virtual_clock() {
        let network = Arc::new(MockUDPNetwork::new());
        let driver = MockRuntime::new();
        let wall_clock_start = std::time::Instant::now();

        driver.block_on(Box::pin(async {
            let (offerer, answerer) = connect_pair(&network).await;

            // The data channel created during `connect_pair` can only open once DTLS has
            // handshaken and SCTP has established its association.
            let mut opened = false;
            for _ in 0..400 {
                if offerer.rec.saw_conn(RTCPeerConnectionState::Connected)
                    && answerer.rec.saw_conn(RTCPeerConnectionState::Connected)
                {
                    opened = true;
                    break;
                }
                advance_both(&offerer.rt, &answerer.rt, Duration::from_millis(25));
            }

            assert!(
                opened,
                "DTLS + SCTP must complete on the virtual clock; offerer {:?}, answerer {:?}",
                offerer.rec.conn_states(),
                answerer.rec.conn_states()
            );
        }));

        assert!(
            wall_clock_start.elapsed() < Duration::from_secs(5),
            "a virtual-clock test must not spend real time: took {:?}",
            wall_clock_start.elapsed()
        );
    }

    /// **The headline claim.** ICE consent freshness (RFC 7675) fails when a peer stops
    /// answering; advancing only the virtual clock must produce that transition, with no
    /// wall-clock time spent waiting for it.
    ///
    /// The answerer's clock is deliberately left frozen: it stops responding to consent
    /// checks not because it is gone but because, from the offerer's point of view, no
    /// answers arrive within the window.
    #[test]
    fn ice_consent_expires_when_only_the_virtual_clock_advances() {
        let network = Arc::new(MockUDPNetwork::new());
        let driver = MockRuntime::new();
        let wall_clock_start = std::time::Instant::now();

        driver.block_on(Box::pin(async {
            let (offerer, answerer) = connect_pair(&network).await;

            // Stop the answerer answering: it can no longer confirm consent.
            answerer.pc.close().await.expect("close answerer");
            settle();

            // Consent has not expired yet: the transition below is produced by advancing the
            // clock, not by closing the peer.
            settle();
            assert!(
                !offerer.rec.saw_ice(RTCIceConnectionState::Disconnected),
                "closing the peer must not by itself disconnect the offerer"
            );

            // Thirty seconds of protocol time, none of it real.
            for _ in 0..120 {
                if offerer.rec.saw_ice(RTCIceConnectionState::Disconnected)
                    || offerer.rec.saw_ice(RTCIceConnectionState::Failed)
                {
                    break;
                }
                offerer.rt.clock().advance(Duration::from_secs(1));
                settle();
            }

            let states = offerer.rec.ice_states();
            assert!(
                states.contains(&RTCIceConnectionState::Disconnected)
                    || states.contains(&RTCIceConnectionState::Failed),
                "consent should have expired once the virtual clock passed the window; saw {states:?}"
            );
        }));

        // The negative half: the whole scenario covered ~2 minutes of protocol time. If any
        // of it had been spent on the wall clock, this would fail — which is exactly how a
        // regression that reintroduced `Instant::now()` in the driver would surface.
        assert!(
            wall_clock_start.elapsed() < Duration::from_secs(5),
            "a virtual-clock test must not spend real time: took {:?}",
            wall_clock_start.elapsed()
        );
    }

    /// Advancing a mock clock moves the instant the core is told, and moves the wall clock
    /// not at all. Removing `Runtime::now()` from the driver breaks the first assertion.
    #[test]
    fn advancing_the_mock_clock_does_not_advance_the_wall_clock() {
        let rt = MockRuntime::new();
        let clock = rt.clock();

        let virtual_before = rt.now();
        let wall_before = std::time::Instant::now();

        clock.advance(Duration::from_secs(30));

        assert_eq!(
            rt.now().duration_since(virtual_before),
            Duration::from_secs(30),
            "the runtime's clock must report exactly what was advanced"
        );
        assert!(
            wall_before.elapsed() < Duration::from_millis(500),
            "advancing virtual time must not sleep: {:?} of real time passed",
            wall_before.elapsed()
        );
    }

    /// Each `MockRuntime` owns its clock, so advancing one leaves the other where it was.
    /// That independence is what lets these tests run in parallel without interfering.
    #[test]
    fn clocks_are_independent_across_runtimes() {
        let a = MockRuntime::new();
        let b = MockRuntime::new();

        let b_before = b.now();
        a.clock().advance(Duration::from_secs(60));

        assert_eq!(
            b.now(),
            b_before,
            "advancing one clock must not move another"
        );
        assert!(a.now() > b.now());
    }

    #[test]
    fn ice_connects_under_a_virtual_clock() {
        let network = Arc::new(MockUDPNetwork::new());
        let driver = MockRuntime::new();
        driver.block_on(Box::pin(async {
            let (offerer, answerer) = connect_pair(&network).await;
            assert!(offerer.rec.saw_ice(RTCIceConnectionState::Connected));
            assert!(answerer.rec.saw_ice(RTCIceConnectionState::Connected));
        }));
    }
}
