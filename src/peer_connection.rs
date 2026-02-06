//! Async peer connection wrapper

use super::ice_gatherer::RTCIceGatherOptions;
use super::*;
use crate::data_channel::DataChannel;
use crate::ice_gatherer::RTCIceGatherer;
use crate::media_track::{TrackLocal, TrackRemote};
use crate::peer_connection_driver::PeerConnectionDriver;
use crate::runtime::Runtime;
use crate::runtime::{Mutex, Receiver, Sender, channel};
use rtc::data_channel::{RTCDataChannelId, RTCDataChannelInit, RTCDataChannelMessage};
use rtc::peer_connection::RTCPeerConnection;
use rtc::peer_connection::configuration::{RTCAnswerOptions, RTCOfferOptions};
use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::rtp_transceiver::{RTCRtpReceiverId, RTCRtpSenderId};
use rtc::sansio::Protocol;
use std::collections::HashMap;
use std::net::SocketAddr;
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
pub(crate) enum InnerMessage {
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
}

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
    pub(crate) msg_tx: Sender<InnerMessage>,
    /// Unified channel receiver for all outgoing messages (taken by driver)
    pub(crate) msg_rx: Mutex<Option<Receiver<InnerMessage>>>,
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

        Ok(Self {
            inner: Arc::new(PeerConnectionInner {
                core: Mutex::new(core),
                runtime,
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
    pub async fn create_offer(
        &self,
        options: Option<RTCOfferOptions>,
    ) -> Result<RTCSessionDescription, Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().await;
        Ok(core.create_offer(options)?)
    }

    /// Create an SDP answer
    pub async fn create_answer(
        &self,
        options: Option<RTCAnswerOptions>,
    ) -> Result<RTCSessionDescription, Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().await;
        Ok(core.create_answer(options)?)
    }

    /// Set the local description
    pub async fn set_local_description(
        &self,
        desc: RTCSessionDescription,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

    /// Set the remote description
    pub async fn set_remote_description(
        &self,
        desc: RTCSessionDescription,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().await;
        core.set_remote_description(desc)?;
        Ok(())
    }

    /// Get the local description
    pub async fn local_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.local_description().cloned()
    }

    /// Get the remote description
    pub async fn remote_description(&self) -> Option<RTCSessionDescription> {
        let core = self.inner.core.lock().await;
        core.remote_description().cloned()
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
    /// # use std::sync::Arc;
    /// # struct Handler;
    /// # #[async_trait::async_trait]
    /// # impl PeerConnectionEventHandler for Handler {}
    /// # async fn example(pc: PeerConnection) -> Result<(), Box<dyn std::error::Error>> {
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
    pub async fn add_ice_candidate(
        &self,
        candidate: RTCIceCandidateInit,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
    /// # async fn example(pc: PeerConnection) -> Result<(), Box<dyn std::error::Error>> {
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
    pub async fn restart_ice(&self) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut core = self.inner.core.lock().await;
            core.restart_ice();
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

    /// Close the peer connection
    pub async fn close(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().await;
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
    /// # use webrtc::RTCDataChannelInit;
    /// # use webrtc::RTCConfigurationBuilder;
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
        options: Option<RTCDataChannelInit>,
    ) -> Result<Arc<DataChannel>, Box<dyn std::error::Error>> {
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

    /// Add a track to the peer connection
    ///
    /// This creates a new media track for sending audio or video.
    /// The track will be negotiated with the remote peer during offer/answer.
    ///
    /// Returns the created TrackLocal that can be used to send RTP packets.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use webrtc::peer_connection::*;
    /// # use webrtc::MediaStreamTrack;
    /// # use webrtc::RTCConfigurationBuilder;
    /// # use std::sync::Arc;
    /// # #[derive(Clone)]
    /// # struct MyHandler;
    /// # #[async_trait::async_trait]
    /// # impl PeerConnectionEventHandler for MyHandler {}
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use rtc::rtp_transceiver::rtp_sender::RtpCodecKind;
    ///
    /// let config = RTCConfigurationBuilder::new().build();
    /// let handler = Arc::new(MyHandler);
    /// let pc = PeerConnection::new(config, handler)?;
    ///
    /// // Create a video track
    /// let track = MediaStreamTrack::new(
    ///     "stream".to_string(),
    ///     "video".to_string(),
    ///     "my-video".to_string(),
    ///     RtpCodecKind::Video,
    ///     vec![],  // Encodings will be added during negotiation
    /// );
    ///
    /// // Add it to the peer connection
    /// let local_track = pc.add_track(track).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn add_track(
        &self,
        track: rtc::media_stream::MediaStreamTrack,
    ) -> Result<Arc<TrackLocal>, Box<dyn std::error::Error>> {
        // Add track via the core
        let sender_id = {
            let mut core = self.inner.core.lock().await;
            core.add_track(track)?
        };

        // Create the local track wrapper
        let local_track = Arc::new(TrackLocal::new(sender_id, self.inner.msg_tx.clone()));

        Ok(local_track)
    }

    /// Bind a UDP socket and create a driver for this peer connection
    ///
    /// The driver must be spawned or awaited to handle I/O and events.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use webrtc::peer_connection::*;
    /// # use webrtc::RTCConfigurationBuilder;
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
    ///     if let Err(e) = driver.run().await {
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
        let driver = PeerConnectionDriver::new(self.inner.clone(), async_socket).await?;

        Ok(driver)
    }
}
