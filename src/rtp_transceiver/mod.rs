//! Async Media API

pub mod rtp_receiver;
pub mod rtp_sender;

use crate::error::Error;
use crate::media_stream::{TrackLocal, TrackRemote};
use crate::peer_connection::{Interceptor, NoopInterceptor, PeerConnectionRef};
use crate::rtp_transceiver::rtp_receiver::RtpReceiverImpl;
use crate::rtp_transceiver::rtp_sender::RtpSenderImpl;
use crate::runtime::Mutex;
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

#[async_trait::async_trait]
pub trait RtpReceiver: Send + Sync + 'static {
    async fn track(&self) -> Result<Arc<dyn TrackRemote>>;
    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>>;
    async fn get_parameters(&mut self) -> Result<RTCRtpReceiveParameters>;
    async fn get_contributing_sources(&self) -> Result<Vec<RTCRtpContributingSource>>;
    async fn get_synchronization_sources(&self) -> Result<Vec<RTCRtpSynchronizationSource>>;
    async fn get_stats(&self, now: Instant) -> Result<RTCStatsReport>;
}

#[async_trait::async_trait]
pub trait RtpSender: Send + Sync + 'static {
    async fn track(&self) -> Result<Arc<dyn TrackLocal>>;
    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>>;
    async fn set_parameters(
        &mut self,
        parameters: RTCRtpSendParameters,
        set_parameter_options: Option<RTCSetParameterOptions>,
    ) -> Result<()>;
    async fn get_parameters(&mut self) -> Result<RTCRtpSendParameters>;
    async fn replace_track(&mut self, track: Arc<dyn TrackLocal>) -> Result<()>;
    async fn set_streams(&mut self, streams: Vec<MediaStreamId>) -> Result<()>;
    async fn get_stats(&self, now: Instant) -> Result<RTCStatsReport>;
}

#[async_trait::async_trait]
pub trait RtpTransceiver: Send + Sync + 'static {
    async fn mid(&self) -> Result<Option<String>>;
    async fn sender(&self) -> Result<Option<Arc<dyn RtpSender>>>;
    async fn receiver(&self) -> Result<Option<Arc<dyn RtpReceiver>>>;
    async fn direction(&self) -> Result<RTCRtpTransceiverDirection>;
    async fn set_direction(&self, direction: RTCRtpTransceiverDirection) -> Result<()>;
    async fn current_direction(&self) -> Result<RTCRtpTransceiverDirection>;
    async fn stop(&self) -> Result<()>;
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
}

#[async_trait::async_trait]
impl<I> RtpTransceiver for RtpTransceiverImpl<I>
where
    I: Interceptor + 'static,
{
    async fn mid(&self) -> Result<Option<String>> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrTPTransceiverNotExisted)?
            .mid()
            .clone())
    }

    async fn sender(&self) -> Result<Option<Arc<dyn RtpSender>>> {
        let mut peer_connection = self.inner.core.lock().await;

        let transceiver = peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrTPTransceiverNotExisted)?;

        let mut sender = self.sender.lock().await;
        if let Some(sender_id) = transceiver.sender() {
            if sender.is_none() {
                *sender = Some(Arc::new(RtpSenderImpl::new(
                    sender_id,
                    Arc::clone(&self.inner),
                )));
            }
        } else {
            *sender = None;
        }

        Ok(sender.clone())
    }

    async fn receiver(&self) -> Result<Option<Arc<dyn RtpReceiver>>> {
        let mut peer_connection = self.inner.core.lock().await;

        let transceiver = peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrTPTransceiverNotExisted)?;

        let mut receiver = self.receiver.lock().await;
        if let Some(receiver_id) = transceiver.receiver() {
            if receiver.is_none() {
                *receiver = Some(Arc::new(RtpReceiverImpl::new(
                    receiver_id,
                    Arc::clone(&self.inner),
                )));
            }
        } else {
            *receiver = None;
        }
        Ok(receiver.clone())
    }

    async fn direction(&self) -> Result<RTCRtpTransceiverDirection> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrTPTransceiverNotExisted)?
            .direction())
    }

    async fn set_direction(&self, direction: RTCRtpTransceiverDirection) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrTPTransceiverNotExisted)?
            .set_direction(direction);

        Ok(())
    }

    async fn current_direction(&self) -> Result<RTCRtpTransceiverDirection> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrTPTransceiverNotExisted)?
            .current_direction())
    }

    async fn stop(&self) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrTPTransceiverNotExisted)?
            .stop()
    }

    async fn set_codec_preferences(&self, codecs: Vec<RTCRtpCodecParameters>) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_transceiver(self.id)
            .ok_or(Error::ErrTPTransceiverNotExisted)?
            .set_codec_preferences(codecs)
    }
}
