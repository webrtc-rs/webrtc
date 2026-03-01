//! Peer connection driver (event loop)
//!
//! Follows the rtc EventLoop pattern with async select

#![allow(clippy::collapsible_if)]

use super::ice_gatherer::{RTCIceGatherer, RTCIceGathererEvent};
use crate::data_channel::{DataChannelEvent, DataChannelImpl};
use crate::media_stream::track_remote::static_rtp::TrackRemoteStaticRTP;
use crate::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use crate::peer_connection::PeerConnectionRef;
use crate::rtp_transceiver::rtp_receiver::RtpReceiverImpl;
use crate::rtp_transceiver::{RtpReceiver, RtpTransceiverImpl};
use crate::runtime::{AsyncUdpSocket, Receiver, channel};
use bytes::BytesMut;
use futures::FutureExt; // For .fuse() in futures::select!
use futures::stream::{FuturesUnordered, StreamExt};
use log::{error, trace};
use rtc::interceptor::{Interceptor, NoopInterceptor};
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::event::{RTCDataChannelEvent, RTCPeerConnectionEvent, RTCTrackEvent};
use rtc::peer_connection::message::RTCMessage;
use rtc::peer_connection::state::RTCIceGatheringState;
use rtc::peer_connection::transport::RTCIceCandidateInit;
use rtc::rtp_transceiver::{RTCRtpReceiverId, RTCRtpSenderId};
use rtc::sansio::Protocol;
use rtc::shared::error::{Error, Result};
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use rtc::{rtcp, rtp};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Capacity of the internal driver event channel (WriteNotify, IceGathering, Close, …).
pub(crate) const PEER_CONNECTION_DRIVER_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Capacity of each data-channel event channel (OnOpen, OnMessage, OnClose, …).
pub(crate) const DATA_CHANNEL_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Capacity of each track-remote event channel (OnMute, OnUnmute, OnEnded, OnRtpPacket, OnRtcpPacket, …).
pub(crate) const TRACK_REMOTE_EVENT_CHANNEL_CAPACITY: usize = 256;

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(86400); // 1 day duration

/// Unified inner message type for the peer connection driver
#[derive(Debug)]
pub(crate) enum PeerConnectionDriverEvent {
    SenderRtp(RTCRtpSenderId, rtp::Packet),
    SenderRtcp(RTCRtpSenderId, Vec<Box<dyn rtcp::Packet>>),
    ReceiverRtcp(RTCRtpReceiverId, Vec<Box<dyn rtcp::Packet>>),
    WriteNotify,
    IceGathering,
    Close,
}

/// The driver for a peer connection
///
/// Runs the event loop following rtc's EventLoop pattern with select!
pub(crate) struct PeerConnectionDriver<I = NoopInterceptor>
where
    I: Interceptor,
{
    inner: Arc<PeerConnectionRef<I>>,
    /// ICE gatherer for managing ICE candidate gathering
    ice_gatherer: RTCIceGatherer,
    sockets: HashMap<SocketAddr, Arc<dyn AsyncUdpSocket>>,
}

impl<I> PeerConnectionDriver<I>
where
    I: Interceptor,
{
    /// Create a new driver for the given peer connection
    pub(crate) async fn new(
        inner: Arc<PeerConnectionRef<I>>,
        ice_gatherer: RTCIceGatherer,
        sockets: HashMap<SocketAddr, Arc<dyn AsyncUdpSocket>>,
    ) -> Result<Self> {
        if sockets.is_empty() {
            return Err(Error::Other("no sockets available".to_owned()));
        }

        Ok(Self {
            inner,
            ice_gatherer,
            sockets,
        })
    }

    /// Run the driver event loop
    ///
    /// This follows rtc Event Loop pattern exactly with select!
    pub(crate) async fn event_loop(
        &mut self,
        mut driver_event_rx: Receiver<PeerConnectionDriverEvent>,
    ) -> Result<()> {
        // Collect socket info into a vec for indexed access
        let socket_list: Vec<(SocketAddr, Arc<dyn AsyncUdpSocket>)> = self
            .sockets
            .iter()
            .map(|(addr, sock)| (*addr, sock.clone()))
            .collect();

        // Pre-allocate buffers once - one per socket, these will be reused forever
        let mut socket_buffers: Vec<Vec<u8>> =
            socket_list.iter().map(|_| vec![0u8; 2000]).collect();

        // Helper function to create a recv future for a specific socket
        let create_socket_recv_future = |idx: usize,
                                         local_addr: SocketAddr,
                                         socket: Arc<dyn AsyncUdpSocket>,
                                         mut buf: Vec<u8>| async move {
            let (n, peer_addr) = socket.recv_from(&mut buf).await?;
            Ok::<_, std::io::Error>((n, local_addr, peer_addr, idx, buf))
        };

        // Create initial set of futures in FuturesUnordered
        let mut socket_recv_futures: FuturesUnordered<_> = socket_list
            .iter()
            .enumerate()
            .map(|(idx, (local_addr, socket))| {
                let buf = std::mem::take(&mut socket_buffers[idx]);
                create_socket_recv_future(idx, *local_addr, socket.clone(), buf).boxed()
            })
            .collect();

        loop {
            // 1.a ice_gatherer poll_write()
            {
                while let Some(msg) = self.ice_gatherer.poll_write() {
                    self.handle_write(msg).await;
                }
            }

            // 1.b peer_connection poll_write() - Send all outgoing packets
            {
                let mut core = self.inner.core.lock().await;
                while let Some(msg) = core.poll_write() {
                    drop(core);
                    self.handle_write(msg).await;
                    core = self.inner.core.lock().await;
                }
            }

            // 2.a ice_gatherer poll_event()
            {
                while let Some(event) = self.ice_gatherer.poll_event() {
                    self.handle_gather_event(event).await;
                }
            }

            // 2.b peer_connection poll_event() - Process all events
            {
                let mut core = self.inner.core.lock().await;
                while let Some(event) = core.poll_event() {
                    drop(core);
                    self.handle_rtc_event(event).await;
                    core = self.inner.core.lock().await;
                }
            }

            // 3.a no need for ice_gatherer poll_read()

            // 3.b peer_connection poll_read() - Process incoming messages
            {
                let mut core = self.inner.core.lock().await;
                while let Some(message) = core.poll_read() {
                    drop(core);
                    self.handle_rtc_message(message).await;
                    core = self.inner.core.lock().await;
                }
            }

            // 4.a poll next timeout
            let mut timeout = {
                let mut core = self.inner.core.lock().await;
                core.poll_timeout()
                    .unwrap_or(Instant::now() + DEFAULT_TIMEOUT_DURATION)
            };
            if let Some(ice_gatherer_timeout) = self.ice_gatherer.poll_timeout() {
                if ice_gatherer_timeout < timeout {
                    timeout = ice_gatherer_timeout;
                }
            }

            let delay_from_now = timeout
                .checked_duration_since(Instant::now())
                .unwrap_or(Duration::from_secs(0));

            // 4.b handle immediate timeout
            if delay_from_now.is_zero() {
                let now = Instant::now();
                self.ice_gatherer.handle_timeout(now)?;
                let mut core = self.inner.core.lock().await;
                core.handle_timeout(now)?;
                continue;
            }

            let timer = crate::runtime::sleep(delay_from_now);
            futures::pin_mut!(timer);

            // Runtime-agnostic select!
            futures::select! {
                // Timer expired
                _ = timer.fuse() => {
                    let now = Instant::now();
                    self.ice_gatherer.handle_timeout(now)?;
                    let mut core = self.inner.core.lock().await;
                    core.handle_timeout(now)?;
                }

                // Driver events (RTP, RTCP, or ICE candidate)
                evt = driver_event_rx.recv().fuse() => {
                    if let Some(evt) = evt {
                        let is_closed = self.handle_driver_event(evt).await;
                        if is_closed {
                            return Ok(());
                        }
                    }
                }

                // Incoming network packet from any socket
                result = socket_recv_futures.next().fuse() => {
                    match result {
                        Some(Ok((n, local_addr, peer_addr, idx, buf))) => {
                            trace!("Received {} bytes from {} to {}", n, peer_addr, local_addr);

                            if let Err(err) = self.handle_read(TaggedBytesMut {
                                now: Instant::now(),
                                transport: TransportContext {
                                    local_addr,
                                    peer_addr,
                                    ecn: None,
                                    transport_protocol: TransportProtocol::UDP,
                                },
                                message: BytesMut::from(&buf[..n]),
                            }).await {
                                 error!("handle_read error: {}", err);
                            }

                            // Immediately create a new future for this socket and reuse the buffer
                            let (socket_local_addr, socket) = &socket_list[idx];
                            socket_recv_futures.push(
                                create_socket_recv_future(idx, *socket_local_addr, socket.clone(), buf).boxed()
                            );
                        }
                        Some(Err(err)) => {
                            error!("Socket recv error: {}", err);
                            //TODO: better handling on socket recv error #777
                            // On error, we lost the buffer, create a new one and restart this socket
                            // This should be rare (only on actual socket errors)
                            // For now, we return the error to stop the loop
                            return Err(err.into());
                        }
                        None => {
                            // All socket futures completed (should never happen in normal operation)
                            return Err(Error::Other("all socket futures completed".to_owned()));
                        }
                    }
                }
            }
        }
    }

    async fn handle_write(&self, msg: TaggedBytesMut) {
        if let Some(socket) = self.sockets.get(&msg.transport.local_addr) {
            match socket.send_to(&msg.message, msg.transport.peer_addr).await {
                Ok(n) => {
                    trace!(
                        "Sent {} bytes to {:?} from {:?}",
                        n, msg.transport.peer_addr, msg.transport.local_addr
                    );
                }
                Err(e) => {
                    error!(
                        "Failed to send to {:?} from {:?}: {}",
                        msg.transport.peer_addr, msg.transport.local_addr, e
                    );
                }
            }
        }
    }

    async fn handle_read(&mut self, msg: TaggedBytesMut) -> Result<()> {
        if self.ice_gatherer.is_ice_message(&msg) {
            self.ice_gatherer.handle_read(msg)?;
        } else {
            let mut core = self.inner.core.lock().await;
            core.handle_read(msg)?;
        }

        Ok(())
    }

    async fn handle_gather_event(&mut self, event: RTCIceGathererEvent) {
        match event {
            RTCIceGathererEvent::LocalIceCandidate(candidate) => {
                trace!("LocalIceCandidate {:?}", candidate);
                let mut core = self.inner.core.lock().await;
                if let Err(err) = core.add_local_candidate(candidate) {
                    error!("Failed to add local candidate: {}", err);
                }
            }
            RTCIceGathererEvent::IceGatheringComplete => {
                let end_of_candidates = RTCIceCandidateInit::default();
                let mut core = self.inner.core.lock().await;
                if let Err(err) = core.add_local_candidate(end_of_candidates) {
                    error!("Failed to add end_of_candidates: {}", err);
                }
            }
        }
    }

    async fn handle_rtc_event(&mut self, event: RTCPeerConnectionEvent) {
        match event {
            RTCPeerConnectionEvent::OnNegotiationNeededEvent => {
                self.inner.handler.on_negotiation_needed().await;
            }
            RTCPeerConnectionEvent::OnIceCandidateEvent(evt) => {
                self.inner.handler.on_ice_candidate(evt).await;
            }
            RTCPeerConnectionEvent::OnIceCandidateErrorEvent(evt) => {
                self.inner.handler.on_ice_candidate_error(evt).await;
            }
            RTCPeerConnectionEvent::OnSignalingStateChangeEvent(state) => {
                self.inner.handler.on_signaling_state_change(state).await;
            }
            RTCPeerConnectionEvent::OnIceConnectionStateChangeEvent(state) => {
                self.inner
                    .handler
                    .on_ice_connection_state_change(state)
                    .await;
            }
            RTCPeerConnectionEvent::OnIceGatheringStateChangeEvent(state) => {
                self.inner
                    .handler
                    .on_ice_gathering_state_change(state)
                    .await;
            }
            RTCPeerConnectionEvent::OnConnectionStateChangeEvent(state) => {
                self.inner.handler.on_connection_state_change(state).await;
            }
            RTCPeerConnectionEvent::OnDataChannel(evt) => {
                let channel_id = match evt {
                    RTCDataChannelEvent::OnOpen(id) => id,
                    RTCDataChannelEvent::OnError(id) => id,
                    RTCDataChannelEvent::OnClosing(id) => id,
                    RTCDataChannelEvent::OnClose(id) => id,
                    RTCDataChannelEvent::OnBufferedAmountLow(id) => id,
                    RTCDataChannelEvent::OnBufferedAmountHigh(id) => id,
                };

                if let RTCDataChannelEvent::OnOpen(_) = &evt {
                    let data_channel_exist = {
                        let mut core = self.inner.core.lock().await;
                        core.data_channel(channel_id).is_some()
                    };

                    if data_channel_exist {
                        let (evt_tx, evt_rx) = channel(DATA_CHANNEL_EVENT_CHANNEL_CAPACITY);
                        let data_channel =
                            Arc::new(DataChannelImpl::new(channel_id, self.inner.clone(), evt_rx));

                        {
                            let mut data_channels = self.inner.data_channel_events_tx.lock().await;
                            if let Entry::Vacant(e) = data_channels.entry(channel_id) {
                                e.insert(evt_tx);
                            }
                        }

                        self.inner.handler.on_data_channel(data_channel).await;
                    }
                }

                let data_channels = self.inner.data_channel_events_tx.lock().await;
                if let Some(evt_tx) = data_channels.get(&channel_id) {
                    let result = match evt {
                        RTCDataChannelEvent::OnOpen(_) => evt_tx.try_send(DataChannelEvent::OnOpen),
                        RTCDataChannelEvent::OnError(_) => {
                            evt_tx.try_send(DataChannelEvent::OnError)
                        }
                        RTCDataChannelEvent::OnClosing(_) => {
                            evt_tx.try_send(DataChannelEvent::OnClosing)
                        }
                        RTCDataChannelEvent::OnClose(_) => {
                            evt_tx.try_send(DataChannelEvent::OnClose)
                        }
                        RTCDataChannelEvent::OnBufferedAmountLow(_) => {
                            evt_tx.try_send(DataChannelEvent::OnBufferedAmountLow)
                        }
                        RTCDataChannelEvent::OnBufferedAmountHigh(_) => {
                            evt_tx.try_send(DataChannelEvent::OnBufferedAmountHigh)
                        }
                    };
                    if let Err(err) = result {
                        error!(
                            "Failed to send RTCDataChannelEvent to data channel {}: {:?}",
                            channel_id, err
                        );
                    }
                } else {
                    error!(
                        "Failed to get data_channel: {:?} for RTCDataChannelEvent",
                        channel_id
                    );
                }
            }
            RTCPeerConnectionEvent::OnTrack(evt) => {
                let track_id = match &evt {
                    RTCTrackEvent::OnOpen(init) => &init.track_id,
                    RTCTrackEvent::OnError(id) => id,
                    RTCTrackEvent::OnClosing(id) => id,
                    RTCTrackEvent::OnClose(id) => id,
                };

                if let RTCTrackEvent::OnOpen(init) = &evt {
                    let (id, track) = {
                        let mut core = self.inner.core.lock().await;
                        (
                            init.receiver_id.into(),
                            core.rtp_receiver(init.receiver_id).map(|receiver| {
                                let track = receiver.track();
                                MediaStreamTrack::new(
                                    track.stream_id().clone(),
                                    track.track_id().clone(),
                                    track.label().clone(),
                                    track.kind(),
                                    vec![],
                                )
                            }),
                        )
                    };

                    if let Some(track) = track {
                        // For simulcast, multiple RTCTrackEvent::OnOpen fire for the same
                        // track_id (one per RID as each layer's first RTP packet arrives).
                        // Only create the TrackRemote and call on_track the first time.
                        let already_open = self
                            .inner
                            .track_remote_events_tx
                            .lock()
                            .await
                            .contains_key(track_id);

                        if !already_open {
                            let (evt_tx, evt_rx) = channel(TRACK_REMOTE_EVENT_CHANNEL_CAPACITY);
                            let track_remote: Arc<dyn TrackRemote> =
                                Arc::new(TrackRemoteStaticRTP::new(
                                    track,
                                    init.receiver_id,
                                    self.inner.driver_event_tx.clone(),
                                    evt_rx,
                                ));

                            {
                                let mut rtp_transceivers = self.inner.rtp_transceivers.lock().await;
                                rtp_transceivers.entry(id).or_insert_with(|| {
                                    Arc::new(RtpTransceiverImpl::new(id, Arc::clone(&self.inner)))
                                });

                                let rtp_transceiver = rtp_transceivers.get(&id).unwrap();

                                let receiver: Arc<dyn RtpReceiver> =
                                    Arc::new(RtpReceiverImpl::new(
                                        id.into(),
                                        Arc::clone(&self.inner),
                                        Arc::clone(&track_remote),
                                    ));
                                rtp_transceiver.set_receiver(Some(receiver)).await;
                            }

                            self.inner
                                .track_remote_events_tx
                                .lock()
                                .await
                                .insert(track_id.clone(), (evt_tx, Arc::clone(&track_remote)));

                            self.inner.handler.on_track(track_remote).await
                        }
                    }
                }

                let track_remotes = self.inner.track_remote_events_tx.lock().await;
                if let Some((evt_tx, track_remote)) = track_remotes.get(track_id) {
                    let (track_id, result) = match evt {
                        RTCTrackEvent::OnOpen(init) => {
                            {
                                let mut core = self.inner.core.lock().await;
                                if let Some(receiver) = core.rtp_receiver(init.receiver_id) {
                                    for coding in
                                        receiver.track().codings().iter().filter(|coding| {
                                            if let Some(ssrc) = coding.rtp_coding_parameters.ssrc
                                                && ssrc == init.ssrc
                                            {
                                                true
                                            } else {
                                                false
                                            }
                                        })
                                    {
                                        track_remote.add_coding(coding.clone()).await;
                                    }
                                }
                            }
                            (
                                init.track_id.clone(),
                                evt_tx.try_send(TrackRemoteEvent::OnOpen(init)),
                            )
                        }
                        RTCTrackEvent::OnError(track_id) => {
                            (track_id, evt_tx.try_send(TrackRemoteEvent::OnError))
                        }
                        RTCTrackEvent::OnClosing(track_id) => {
                            (track_id, evt_tx.try_send(TrackRemoteEvent::OnEnding))
                        }
                        RTCTrackEvent::OnClose(track_id) => {
                            (track_id, evt_tx.try_send(TrackRemoteEvent::OnEnded))
                        }
                    };
                    if let Err(err) = result {
                        error!(
                            "Failed to send RTCTrackEvent to track remote {}: {:?}",
                            track_id, err
                        );
                    }
                } else {
                    error!(
                        "Failed to get track_remote: {:?} for RTCTrackEvent",
                        track_id
                    );
                }
            }
        }
    }

    async fn handle_rtc_message(&mut self, message: RTCMessage) {
        match message {
            RTCMessage::DataChannelMessage(channel_id, dc_message) => {
                let data_channels = self.inner.data_channel_events_tx.lock().await;
                if let Some(evt_tx) = data_channels.get(&channel_id) {
                    if let Err(err) = evt_tx.try_send(DataChannelEvent::OnMessage(dc_message)) {
                        error!(
                            "Failed to send DataChannelMessage to data channel {}: {:?}",
                            channel_id, err
                        );
                    }
                } else {
                    error!(
                        "Failed to get data_channel: {:?} for DataChannelMessage",
                        channel_id
                    );
                }
            }
            RTCMessage::RtpPacket(track_id, packet) => {
                let track_remotes = self.inner.track_remote_events_tx.lock().await;
                if let Some(evt_tx) = track_remotes.get(&track_id) {
                    if let Err(err) = evt_tx.0.try_send(TrackRemoteEvent::OnRtpPacket(packet)) {
                        error!(
                            "Failed to send RtpPacket to track remote {}: {:?}",
                            track_id, err
                        );
                    }
                } else {
                    error!("Failed to get track_remote: {:?} for RtpPacket", track_id);
                }
            }
            RTCMessage::RtcpPacket(track_id, packets) => {
                let track_remotes = self.inner.track_remote_events_tx.lock().await;
                if let Some(evt_tx) = track_remotes.get(&track_id) {
                    if let Err(err) = evt_tx.0.try_send(TrackRemoteEvent::OnRtcpPacket(packets)) {
                        error!(
                            "Failed to send RtcpPacket to track remote {}: {:?}",
                            track_id, err
                        );
                    }
                } else {
                    error!("Failed to get track_remote: {:?} for RtcpPacket", track_id);
                }
            }
        }
    }

    async fn handle_driver_event(&mut self, evt: PeerConnectionDriverEvent) -> bool {
        match evt {
            PeerConnectionDriverEvent::SenderRtp(sender_id, packet) => {
                let mut core = self.inner.core.lock().await;
                if let Some(mut sender) = core.rtp_sender(sender_id) {
                    if let Err(err) = sender.write_rtp(packet) {
                        error!("Failed to send RTP: {}", err);
                    }
                } else {
                    error!(
                        "Failed to send RTP due to unknown sender id {:?}",
                        sender_id
                    );
                }
            }
            PeerConnectionDriverEvent::SenderRtcp(sender_id, rtcp_packets) => {
                let mut core = self.inner.core.lock().await;
                if let Some(mut sender) = core.rtp_sender(sender_id) {
                    if let Err(err) = sender.write_rtcp(rtcp_packets) {
                        error!("Failed to send RTCP: {}", err);
                    }
                } else {
                    error!(
                        "Failed to send RTCP feedback due to unknown sender id {:?}",
                        sender_id
                    );
                }
            }
            PeerConnectionDriverEvent::ReceiverRtcp(receiver_id, rtcp_packets) => {
                let mut core = self.inner.core.lock().await;
                if let Some(mut receiver) = core.rtp_receiver(receiver_id) {
                    if let Err(err) = receiver.write_rtcp(rtcp_packets) {
                        error!("Failed to send RTCP feedback: {}", err);
                    }
                } else {
                    error!(
                        "Failed to send RTCP feedback due to unknown receiver id {:?}",
                        receiver_id
                    );
                }
            }
            PeerConnectionDriverEvent::WriteNotify => {
                //Do nothing, just want to wake up from futures::select! in order to poll_write
            }
            PeerConnectionDriverEvent::IceGathering => {
                if self.ice_gatherer.state() != RTCIceGatheringState::Gathering {
                    if let Err(err) = self.ice_gatherer.gather().await {
                        error!("Failed to gather ice gathering: {}", err);
                    }
                }
            }
            PeerConnectionDriverEvent::Close => return true,
        }

        false
    }
}
