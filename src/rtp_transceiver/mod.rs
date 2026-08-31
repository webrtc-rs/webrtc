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

// Async RTP receiver/sender implementations. Both modules contain only crate-internal
// types (`RtpReceiverImpl`/`RtpSenderImpl`); the public API is the `RtpReceiver` and
// `RtpSender` traits defined below, handed out as `Arc<dyn ...>`.
pub(crate) mod rtp_receiver;
pub(crate) mod rtp_sender;

use crate::error::Error;
use crate::media_stream::track_local::TrackLocalContext;
use crate::media_stream::{track_local::TrackLocal, track_remote::TrackRemote};
use crate::peer_connection::PeerConnectionRef;
use crate::peer_connection::driver::DRIVER_TO_TRACK_LOCAL_EVENT_CHANNEL_CAPACITY;
use crate::peer_connection::transport::DtlsTransport;
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
///
/// Every `async` method here is a round-trip to the background driver that owns the connection
/// state, so all of them are fallible: they return an error once this receiver or its peer
/// connection is gone. That is the only failure mode unless a method documents another, and it
/// is not repeated on each one. [`id`](RtpReceiver::id) and [`track`](RtpReceiver::track) are
/// infallible, because the handle caches them.
#[async_trait::async_trait]
pub trait RtpReceiver: crate::sealed::Sealed + Send + Sync + 'static {
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
    /// The DTLS transport over which this receiver's RTP packets are received.
    ///
    /// `Ok(None)` until this receiver's transceiver has been associated by negotiation — the spec
    /// sources this from the per-receiver `[[ReceiverTransport]]` slot, which is filled while
    /// applying a local or remote description. Under bundling every sender and receiver shares
    /// one transport, so all of them compare equal by `id()`.
    ///
    /// `Err` means something different from `Ok(None)`: the receiver itself no longer exists.
    /// Note this is *not* keyed on the DTLS handshake having started — see
    /// [`docs/transport-objects.md`](https://github.com/webrtc-rs/webrtc/blob/master/docs/transport-objects.md).
    ///
    /// # Specification
    ///
    /// * [W3C](https://www.w3.org/TR/webrtc/#dom-rtcrtpreceiver-transport)
    async fn transport(&self) -> Result<Option<Arc<dyn DtlsTransport>>>;
}

/// An RTP Sender that sends media to a remote peer.
///
/// Every `async` method here is a round-trip to the background driver that owns the connection
/// state, so all of them are fallible: they return an error once this sender or its peer
/// connection is gone. That is the only failure mode unless a method documents another, and it
/// is not repeated on each one. [`id`](RtpSender::id) and [`track`](RtpSender::track) are
/// infallible, because the handle caches them.
#[async_trait::async_trait]
pub trait RtpSender: crate::sealed::Sealed + Send + Sync + 'static {
    /// Returns the unique ID of the RTP sender.
    fn id(&self) -> RTCRtpSenderId;
    /// Returns the local track associated with this sender.
    fn track(&self) -> &Arc<dyn TrackLocal>;
    /// Returns the capabilities of the sender for the given codec kind.
    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>>;
    /// Sets the parameters for this sender.
    ///
    /// # Errors
    ///
    /// Also returns an error if `parameters` does not match the encodings the transceiver was
    /// created with — the W3C algorithm rejects changes to the number or order of encodings.
    async fn set_parameters(
        &self,
        parameters: RTCRtpSendParameters,
        set_parameter_options: Option<RTCSetParameterOptions>,
    ) -> Result<()>;
    /// Returns the current parameters configured for this sender.
    async fn get_parameters(&self) -> Result<RTCRtpSendParameters>;
    /// Replaces the track currently being sent by this sender.
    ///
    /// Does not trigger renegotiation when the new track is compatible with the negotiated
    /// codec.
    ///
    /// # Errors
    ///
    /// Also returns an error if the new track's kind differs from the current one.
    async fn replace_track(&self, track: Arc<dyn TrackLocal>) -> Result<()>;
    /// Sets the media streams associated with this sender's track.
    async fn set_streams(&self, streams: Vec<MediaStreamId>) -> Result<()>;
    /// Returns a statistics report for this sender.
    async fn get_stats(&self, now: Instant) -> Result<RTCStatsReport>;
    /// The DTLS transport over which this sender's RTP packets are sent.
    ///
    /// `Ok(None)` until this sender's transceiver has been associated by negotiation — the spec
    /// sources this from the per-sender `[[SenderTransport]]` slot, which is filled while
    /// applying a local or remote description. Under bundling every sender and receiver shares
    /// one transport, so all of them compare equal by `id()`.
    ///
    /// `Err` means something different from `Ok(None)`: the sender itself no longer exists.
    /// Note this is *not* keyed on the DTLS handshake having started — see
    /// [`docs/transport-objects.md`](https://github.com/webrtc-rs/webrtc/blob/master/docs/transport-objects.md).
    ///
    /// # Specification
    ///
    /// * [W3C](https://www.w3.org/TR/webrtc/#dom-rtcrtpsender-transport)
    async fn transport(&self) -> Result<Option<Arc<dyn DtlsTransport>>>;
}

/// An RTP Transceiver that represents a combination of an RTP Sender and Receiver.
///
/// Every `async` method here is a round-trip to the background driver that owns the connection
/// state, so all of them are fallible: they return an error once this transceiver or its peer
/// connection is gone. That is the only failure mode unless a method documents another, and it
/// is not repeated on each one. [`id`](RtpTransceiver::id) is infallible, because the handle
/// caches it.
#[async_trait::async_trait]
pub trait RtpTransceiver: crate::sealed::Sealed + Send + Sync + 'static {
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
    ///
    /// Takes effect at the next negotiation; read the negotiated value back with
    /// [`current_direction`](RtpTransceiver::current_direction).
    ///
    /// # Errors
    ///
    /// Also returns an error if the transceiver has been stopped.
    async fn set_direction(&self, direction: RTCRtpTransceiverDirection) -> Result<()>;
    /// Returns the current direction negotiated for this transceiver.
    async fn current_direction(&self) -> Result<RTCRtpTransceiverDirection>;
    /// Permanently stops the transceiver.
    ///
    /// Irreversible: the transceiver sends and receives no further media. Triggers
    /// renegotiation.
    async fn stop(&self) -> Result<()>;
    /// Sets the preferred codecs for this transceiver.
    ///
    /// # Errors
    ///
    /// Also returns an error if any codec is not registered in the media engine, or does not
    /// match the transceiver's kind.
    async fn set_codec_preferences(&self, codecs: Vec<RTCRtpCodecParameters>) -> Result<()>;
}

/// Concrete async rtp transceiver implementation (generic over interceptor type).
///
/// This wraps a rtp transceiver and provides async send/receive APIs.
pub(crate) struct RtpTransceiverImpl {
    /// Unique identifier for this rtp transceiver
    id: RTCRtpTransceiverId,

    /// Inner PeerConnection Reference
    inner: Arc<PeerConnectionRef>,

    sender: Mutex<Option<Arc<dyn RtpSender>>>,
    receiver: Mutex<Option<Arc<dyn RtpReceiver>>>,
}

impl RtpTransceiverImpl {
    /// Create a new rtp transceiver wrapper
    pub(crate) fn new(id: RTCRtpTransceiverId, inner: Arc<PeerConnectionRef>) -> Self {
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
            let (evt_tx, evt_rx) = channel(DRIVER_TO_TRACK_LOCAL_EVENT_CHANNEL_CAPACITY);
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

impl crate::sealed::Sealed for RtpTransceiverImpl {}

#[async_trait::async_trait]
impl RtpTransceiver for RtpTransceiverImpl {
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
