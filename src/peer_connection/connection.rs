//! Async peer connection wrapper

use super::*;
use crate::data_channel::DataChannel;
use crate::runtime::Runtime;
use crate::track::TrackRemote;
use rtc::data_channel::{RTCDataChannelId, RTCDataChannelMessage};
use rtc::peer_connection::RTCPeerConnection;
use rtc::peer_connection::configuration::{RTCAnswerOptions, RTCOfferOptions};
use rtc::rtp::packet::Packet as RtpPacket;
use rtc::rtp_transceiver::RTCRtpReceiverId;
use rtc::sansio::Protocol;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
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
    /// ICE servers for STUN/TURN gathering
    pub(crate) ice_servers: Vec<RTCIceServer>,
    /// Local socket address (set after bind)
    pub(crate) local_addr: Mutex<Option<SocketAddr>>,
    /// Waker for the driver task (to wake it when events are added asynchronously)
    pub(crate) driver_waker: Mutex<Option<std::task::Waker>>,
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
    /// Remote tracks (incoming media)
    pub(crate) remote_tracks: Mutex<HashMap<RTCRtpReceiverId, Arc<TrackRemote>>>,
    /// RTP packet senders for remote tracks
    pub(crate) track_rxs: Mutex<HashMap<RTCRtpReceiverId, mpsc::UnboundedSender<RtpPacket>>>,
    /// Channel for outgoing RTP packets
    pub(crate) rtp_tx: mpsc::UnboundedSender<crate::track::OutgoingRtpPacket>,
    /// Channel for receiving outgoing RTP packets (taken by driver)
    pub(crate) rtp_rx: Mutex<Option<mpsc::UnboundedReceiver<crate::track::OutgoingRtpPacket>>>,
    /// Channel for outgoing RTCP packets from senders
    pub(crate) rtcp_tx: mpsc::UnboundedSender<crate::track::OutgoingRtcpPackets>,
    /// Channel for receiving outgoing RTCP packets from senders (taken by driver)
    pub(crate) rtcp_rx: Mutex<Option<mpsc::UnboundedReceiver<crate::track::OutgoingRtcpPackets>>>,
    /// Channel for outgoing RTCP packets from receivers
    pub(crate) receiver_rtcp_tx: mpsc::UnboundedSender<crate::track::OutgoingReceiverRtcpPackets>,
    /// Channel for receiving outgoing RTCP packets from receivers (taken by driver)
    pub(crate) receiver_rtcp_rx:
        Mutex<Option<mpsc::UnboundedReceiver<crate::track::OutgoingReceiverRtcpPackets>>>,
}

// Safety: we protect it with Mutex to make it Send + Sync
unsafe impl Send for PeerConnectionInner {}
unsafe impl Sync for PeerConnectionInner {}

impl PeerConnectionInner {
    /// Wake the driver task to process pending events
    ///
    /// This is called when async tasks add events to the core (e.g., ICE candidates)
    /// to ensure the driver polls and dispatches them to the event handler.
    pub(crate) fn wake_driver(&self) {
        let waker_opt = self.driver_waker.lock().unwrap().take();
        if let Some(waker) = waker_opt {
            eprintln!("🔔 Waking driver with waker.wake() (consumes waker)");
            waker.wake();  // Use wake() instead of wake_by_ref()
            eprintln!("🔔 Driver waker.wake() called");
        } else {
            eprintln!("⚠️  No driver waker available!");
        }
    }
}

impl PeerConnection {
    /// Create a new peer connection with a custom runtime
    pub fn new_with_runtime(
        runtime: Arc<dyn Runtime>,
        config: RTCConfiguration,
        handler: Arc<dyn PeerConnectionEventHandler>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Store ice_servers for STUN/TURN gathering
        let ice_servers = config.ice_servers().to_vec();
        let core = RTCPeerConnection::new(config)?;

        // Create channel for data channel messages
        let (data_tx, data_rx) = mpsc::unbounded_channel();

        // Create channel for RTP packets
        let (rtp_tx, rtp_rx) = mpsc::unbounded_channel();

        // Create channel for RTCP packets from senders
        let (rtcp_tx, rtcp_rx) = mpsc::unbounded_channel();

        // Create channel for RTCP packets from receivers
        let (receiver_rtcp_tx, receiver_rtcp_rx) = mpsc::unbounded_channel();

        Ok(Self {
            inner: Arc::new(PeerConnectionInner {
                core: Mutex::new(core),
                runtime,
                handler,
                ice_servers,
                local_addr: Mutex::new(None),
                driver_waker: Mutex::new(None),
                data_channels: Mutex::new(HashMap::new()),
                data_channel_rxs: Mutex::new(HashMap::new()),
                data_tx,
                data_rx: Mutex::new(Some(data_rx)),
                remote_tracks: Mutex::new(HashMap::new()),
                track_rxs: Mutex::new(HashMap::new()),
                rtp_tx,
                rtp_rx: Mutex::new(Some(rtp_rx)),
                rtcp_tx,
                rtcp_rx: Mutex::new(Some(rtcp_rx)),
                receiver_rtcp_tx,
                receiver_rtcp_rx: Mutex::new(Some(receiver_rtcp_rx)),
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
        {
            let mut core = self.inner.core.lock().unwrap();
            core.set_local_description(desc)?;
        }
        
        // Trigger ICE candidate gathering
        if let Some(local_addr) = *self.inner.local_addr.lock().unwrap() {
            let inner = self.inner.clone();
            
            // Gather host candidates (synchronous)
            let host_candidates = crate::ice_gatherer::gather_host_candidates(local_addr);
            
            // Add host candidates to rtc core
            for candidate_init in host_candidates {
                let mut core = inner.core.lock().unwrap();
                if let Err(e) = core.add_local_candidate(candidate_init) {
                    log::warn!("Failed to add host candidate: {}", e);
                }
            }
            
            // Spawn background task for STUN gathering (server reflexive candidates)
            if !inner.ice_servers.is_empty() {
                eprintln!("🚀 Spawning STUN gathering task with {} ice_servers", inner.ice_servers.len());
                let ice_servers = inner.ice_servers.clone();
                let inner_clone = inner.clone();
                
                self.inner.runtime.spawn(Box::pin(async move {
                    eprintln!("🔄 STUN gathering task started, core ptr: {:p}", &*inner_clone.core.lock().unwrap());
                    let srflx_candidates = crate::ice_gatherer::gather_srflx_candidates(
                        local_addr,
                        &ice_servers,
                    ).await;
                    
                    eprintln!("📥 STUN gathering returned {} candidates", srflx_candidates.len());
                    
                    // Add srflx candidates to rtc core
                    {
                        let mut core = inner_clone.core.lock().unwrap();
                        eprintln!("🔄 STUN task acquired core lock, ptr: {:p}", &*core);
                        for candidate_init in srflx_candidates {
                            eprintln!("🔍 Adding srflx candidate to core: {:?}", candidate_init.candidate);
                            match core.add_local_candidate(candidate_init) {
                                Ok(()) => {
                                    eprintln!("✅ add_local_candidate returned Ok - event should be queued");
                                },
                                Err(e) => {
                                    log::warn!("Failed to add srflx candidate: {}", e);
                                    eprintln!("❌ add_local_candidate returned Err: {}", e);
                                }
                            }
                        }
                        
                        // Try to flush events by calling handle_timeout
                        if let Err(e) = core.handle_timeout(Instant::now()) {
                            eprintln!("⚠️  handle_timeout error: {}", e);
                        }
                        eprintln!("🔓 STUN task releasing core lock");
                    }
                    
                    // Wake the driver IMMEDIATELY to process the new candidate events
                    eprintln!("🔔 Waking driver to process srflx candidate events");
                    inner_clone.wake_driver();
                    eprintln!("🏁 STUN gathering task complete");
                }));
            } else {
                eprintln!("ℹ️  No ICE servers configured, skipping STUN gathering");
            }
        }
        
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

    /// Add an ICE candidate received from the remote peer
    ///
    /// This method adds a remote ICE candidate received through the signaling channel.
    /// The remote description must be set before calling this method.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use webrtc::peer_connection::{PeerConnection, RTCIceCandidateInit};
    /// # use std::sync::Arc;
    /// # struct Handler;
    /// # #[async_trait::async_trait]
    /// # impl webrtc::peer_connection::PeerConnectionEventHandler for Handler {}
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
    /// pc.add_ice_candidate(candidate_init)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_ice_candidate(
        &self,
        candidate: RTCIceCandidateInit,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().unwrap();
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
    /// pc.restart_ice()?;
    ///
    /// // Create new offer with new ICE credentials
    /// let offer = pc.create_offer(None)?;
    /// pc.set_local_description(offer.clone())?;
    /// // Send offer to remote peer through signaling
    /// # Ok(())
    /// # }
    /// ```
    pub fn restart_ice(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut core = self.inner.core.lock().unwrap();
        core.restart_ice();
        Ok(())
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
    /// # use webrtc::track::MediaStreamTrack;
    /// # use std::sync::Arc;
    /// # #[derive(Clone)]
    /// # struct MyHandler;
    /// # #[async_trait::async_trait]
    /// # impl PeerConnectionEventHandler for MyHandler {}
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use rtc::media_stream::MediaStreamTrack;
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
    ) -> Result<Arc<crate::track::TrackLocal>, Box<dyn std::error::Error>> {
        // Add track via the core
        let sender_id = {
            let mut core = self.inner.core.lock().unwrap();
            core.add_track(track)?
        };

        // Create the local track wrapper
        let local_track = Arc::new(crate::track::TrackLocal::new(
            sender_id,
            self.inner.rtp_tx.clone(),
            self.inner.rtcp_tx.clone(),
        ));

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
