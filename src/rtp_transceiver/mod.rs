//! RTP Transceiver, Sender, and Receiver API
//!
//! This module provides the [`RtpTransceiver`], [`RtpSender`], and [`RtpReceiver`] traits, which
//! manage the sending and receiving of media tracks over a peer connection.
//!
//! # Concepts
//!
//! *   **[`RtpTransceiver`]**: Represents a combination of an RTP sender and receiver that share a
//!     common media ID (MID) and SDP media section (`m=`).
//! *   **[`RtpSender`]**: Manages the transmission of a local media track ([`TrackLocal`])
//!     to the remote peer.
//! *   **[`RtpReceiver`]**: Manages the reception of a remote media track ([`TrackRemote`])
//!     from the remote peer.
//!
//! Senders, receivers, and transceivers are created automatically when adding tracks or transceivers
//! to the peer connection, or when negotiation completes.
//!
//! # Examples
//!
//! ## Controlling Transceiver Direction
//!
//! ```no_run
//! # use webrtc::rtp_transceiver::{RtpTransceiver, RTCRtpTransceiverDirection};
//! # use std::sync::Arc;
//! # async fn configure_transceiver(transceiver: Arc<dyn RtpTransceiver>) -> webrtc::error::Result<()> {
//! // Set preferred direction to receive only
//! transceiver.set_direction(RTCRtpTransceiverDirection::Recvonly).await?;
//!
//! // Check the current negotiated direction
//! let current = transceiver.current_direction().await?;
//! println!("Negotiated direction: {:?}", current);
//! # Ok(())
//! # }
//! ```

/// Async RTP Receiver API.
pub mod rtp_receiver;
/// Async RTP Sender API.
pub mod rtp_sender;

use crate::error::Error;
use crate::media_stream::track_local::TrackLocalContext;
use crate::media_stream::{track_local::TrackLocal, track_remote::TrackRemote};
use crate::peer_connection::driver::TRACK_LOCAL_EVENT_CHANNEL_CAPACITY;
use crate::peer_connection::{Interceptor, NoopInterceptor, PeerConnectionRef};
use crate::runtime::Mutex;
use crate::runtime::channel;
use rtc::media_stream::MediaStreamId;
use rtc::rtp_transceiver::RTCRtpTransceiverId;
use rtc::rtp_transceiver::rtp_receiver::{RTCRtpContributingSource, RTCRtpSynchronizationSource};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCapabilities, RTCRtpCodecParameters, RTCRtpReceiveParameters, RTCRtpSendParameters,
    RTCSetParameterOptions, RtpCodecKind,
};
pub use rtc::rtp_transceiver::{
    RTCRtpReceiverId, RTCRtpSenderId, RTCRtpTransceiverDirection, RTCRtpTransceiverInit,
};
use rtc::shared::error::Result;
use rtc::statistics::report::RTCStatsReport;
use std::sync::Arc;
use std::time::Instant;

/// An RTP Receiver that receives media from a remote peer.
#[async_trait::async_trait]
pub trait RtpReceiver: Send + Sync + 'static {
    /// Returns the unique ID of the RTP receiver.
    fn id(&self) -> RTCRtpReceiverId;
    /// Returns the remote track associated with this receiver.
    fn track(&self) -> &Arc<dyn TrackRemote>;
    /// Returns the capabilities of the receiver for the given codec kind.
    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>>;
    /// Returns the current parameters configured for this receiver.
    async fn get_parameters(&self) -> Result<RTCRtpReceiveParameters>;
    /// Returns the contributing sources (CSRC) for this receiver.
    async fn get_contributing_sources(&self) -> Result<Vec<RTCRtpContributingSource>>;
    /// Returns the synchronization sources (SSRC) for this receiver.
    async fn get_synchronization_sources(&self) -> Result<Vec<RTCRtpSynchronizationSource>>;
    /// Returns a statistics report for this receiver.
    async fn get_stats(&self, now: Instant) -> Result<RTCStatsReport>;
}

/// An RTP Sender that sends media to a remote peer.
#[async_trait::async_trait]
pub trait RtpSender: Send + Sync + 'static {
    /// Returns the unique ID of the RTP sender.
    fn id(&self) -> RTCRtpSenderId;
    /// Returns the local track associated with this sender.
    fn track(&self) -> &Arc<dyn TrackLocal>;
    /// Returns the capabilities of the sender for the given codec kind.
    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>>;
    /// Sets the parameters for this sender.
    async fn set_parameters(
        &self,
        parameters: RTCRtpSendParameters,
        set_parameter_options: Option<RTCSetParameterOptions>,
    ) -> Result<()>;
    /// Returns the current parameters configured for this sender.
    async fn get_parameters(&self) -> Result<RTCRtpSendParameters>;
    /// Replaces the track currently being sent by this sender.
    async fn replace_track(&self, track: Arc<dyn TrackLocal>) -> Result<()>;
    /// Sets the media streams associated with this sender's track.
    async fn set_streams(&self, streams: Vec<MediaStreamId>) -> Result<()>;
    /// Returns a statistics report for this sender.
    async fn get_stats(&self, now: Instant) -> Result<RTCStatsReport>;
}

/// An RTP Transceiver that represents a combination of an RTP Sender and Receiver.
#[async_trait::async_trait]
pub trait RtpTransceiver: Send + Sync + 'static {
    /// Returns the unique ID of the transceiver.
    fn id(&self) -> RTCRtpTransceiverId;
    /// Returns the media ID (MID) assigned to this transceiver.
    async fn mid(&self) -> Result<Option<String>>;
    /// Returns the sender associated with this transceiver, if any.
    async fn sender(&self) -> Result<Option<Arc<dyn RtpSender>>>;
    /// Returns the receiver associated with this transceiver, if any.
    async fn receiver(&self) -> Result<Option<Arc<dyn RtpReceiver>>>;
    /// Returns the preferred direction configured for this transceiver.
    async fn direction(&self) -> Result<RTCRtpTransceiverDirection>;
    /// Sets the preferred direction for this transceiver.
    async fn set_direction(&self, direction: RTCRtpTransceiverDirection) -> Result<()>;
    /// Returns the current direction negotiated for this transceiver.
    async fn current_direction(&self) -> Result<RTCRtpTransceiverDirection>;
    /// Permanently stops the transceiver.
    async fn stop(&self) -> Result<()>;
    /// Sets the preferred codecs for this transceiver.
    async fn set_codec_preferences(&self, codecs: Vec<RTCRtpCodecParameters>) -> Result<()>;
}

/// Concrete async rtp transceiver implementation (generic over interceptor type).
///
/// This wraps a rtp transceiver and provides async send/receive APIs.
pub(crate) struct RtpTransceiverImpl<I = NoopInterceptor>
where
    I: Interceptor,
{
    /// Unique identifier for this rtp transceiver
    id: RTCRtpTransceiverId,

    /// Inner PeerConnection Reference
    inner: Arc<PeerConnectionRef<I>>,

    sender: Mutex<Option<Arc<dyn RtpSender>>>,
    receiver: Mutex<Option<Arc<dyn RtpReceiver>>>,
}

impl<I> RtpTransceiverImpl<I>
where
    I: Interceptor,
{
    /// Create a new rtp transceiver wrapper
    pub(crate) fn new(id: RTCRtpTransceiverId, inner: Arc<PeerConnectionRef<I>>) -> Self {
        Self {
            id,
            inner,
            sender: Mutex::new(None),
            receiver: Mutex::new(None),
        }
    }

    pub(crate) async fn set_sender(&self, rtp_sender: Option<Arc<dyn RtpSender>>) {
        let mut sender = self.sender.lock().await;

        if let Some(rtp_sender) = sender.take() {
            let track_id = rtp_sender.track().track_id().await;
            self.inner
                .track_local_events_tx
                .lock()
                .await
                .remove(&track_id);
            rtp_sender.track().unbind().await;
        }

        if let Some(rtp_sender) = rtp_sender
            && let Ok(params) = rtp_sender.get_parameters().await
        {
            // Wire an event channel so RTCP feedback the remote sends about this track
            // (Receiver Reports, PLI/FIR) can be read via `TrackLocal::poll`. The driver
            // routes inbound RTCP tagged with this track id to `evt_tx`.
            let track_id = rtp_sender.track().track_id().await;
            let (evt_tx, evt_rx) = channel(TRACK_LOCAL_EVENT_CHANNEL_CAPACITY);
            self.inner
                .track_local_events_tx
                .lock()
                .await
                .insert(track_id, evt_tx);
            rtp_sender
                .track()
                .bind(
                    TrackLocalContext {
                        rtp_sender_id: self.id.into(),
                        rtp_parameters: params.rtp_parameters,
                        driver_event_tx: self.inner.driver_event_tx.clone(),
                    },
                    evt_rx,
                )
                .await;
            *sender = Some(rtp_sender);
        }
    }

    pub(crate) async fn set_receiver(&self, rtp_receiver: Option<Arc<dyn RtpReceiver>>) {
        let mut receiver = self.receiver.lock().await;
        *receiver = rtp_receiver;
    }
}

#[async_trait::async_trait]
impl<I> RtpTransceiver for RtpTransceiverImpl<I>
where
    I: Interceptor + 'static,
{
    fn id(&self) -> RTCRtpTransceiverId {
        self.id
    }

    async fn mid(&self) -> Result<Option<String>> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrRTPTransceiverNotExisted)?
            .mid()
            .clone())
    }

    async fn sender(&self) -> Result<Option<Arc<dyn RtpSender>>> {
        {
            let mut peer_connection = self.inner.core.lock().await;
            let _ = peer_connection
                .rtp_transceiver(self.id)
                .ok_or(Error::ErrRTPTransceiverNotExisted)?;
        }

        let sender = self.sender.lock().await;
        Ok(sender.clone())
    }

    async fn receiver(&self) -> Result<Option<Arc<dyn RtpReceiver>>> {
        {
            let mut peer_connection = self.inner.core.lock().await;

            let _ = peer_connection
                .rtp_transceiver(self.id)
                .ok_or(Error::ErrRTPTransceiverNotExisted)?;
        }

        let receiver = self.receiver.lock().await;
        Ok(receiver.clone())
    }

    async fn direction(&self) -> Result<RTCRtpTransceiverDirection> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrRTPTransceiverNotExisted)?
            .direction())
    }

    async fn set_direction(&self, direction: RTCRtpTransceiverDirection) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrRTPTransceiverNotExisted)?
            .set_direction(direction);

        Ok(())
    }

    async fn current_direction(&self) -> Result<RTCRtpTransceiverDirection> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrRTPTransceiverNotExisted)?
            .current_direction())
    }

    async fn stop(&self) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrRTPTransceiverNotExisted)?
            .stop()
    }

    async fn set_codec_preferences(&self, codecs: Vec<RTCRtpCodecParameters>) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrRTPTransceiverNotExisted)?
            .set_codec_preferences(codecs)
    }
}
