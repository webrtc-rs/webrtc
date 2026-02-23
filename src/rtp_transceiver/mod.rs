//! Async Media API

pub mod rtp_receiver;
pub mod rtp_sender;

use crate::error::Error;
use crate::media_stream::track_local::TrackLocalContext;
use crate::media_stream::{track_local::TrackLocal, track_remote::TrackRemote};
use crate::peer_connection::{Interceptor, NoopInterceptor, PeerConnectionRef};
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
    fn id(&self) -> RTCRtpReceiverId;
    fn track(&self) -> &Arc<dyn TrackRemote>;
    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>>;
    async fn get_parameters(&self) -> Result<RTCRtpReceiveParameters>;
    async fn get_contributing_sources(&self) -> Result<Vec<RTCRtpContributingSource>>;
    async fn get_synchronization_sources(&self) -> Result<Vec<RTCRtpSynchronizationSource>>;
    async fn get_stats(&self, now: Instant) -> Result<RTCStatsReport>;
}

#[async_trait::async_trait]
pub trait RtpSender: Send + Sync + 'static {
    fn id(&self) -> RTCRtpSenderId;
    fn track(&self) -> &Arc<dyn TrackLocal>;
    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>>;
    async fn set_parameters(
        &self,
        parameters: RTCRtpSendParameters,
        set_parameter_options: Option<RTCSetParameterOptions>,
    ) -> Result<()>;
    async fn get_parameters(&self) -> Result<RTCRtpSendParameters>;
    async fn replace_track(&self, track: Arc<dyn TrackLocal>) -> Result<()>;
    async fn set_streams(&self, streams: Vec<MediaStreamId>) -> Result<()>;
    async fn get_stats(&self, now: Instant) -> Result<RTCStatsReport>;
}

#[async_trait::async_trait]
pub trait RtpTransceiver: Send + Sync + 'static {
    fn id(&self) -> RTCRtpTransceiverId;
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

    pub(crate) async fn set_sender(&self, rtp_sender: Option<Arc<dyn RtpSender>>) {
        let mut sender = self.sender.lock().await;

        if let Some(rtp_sender) = sender.take() {
            rtp_sender.track().unbind().await;
        }

        if let Some(rtp_sender) = rtp_sender {
            rtp_sender
                .track()
                .bind(TrackLocalContext {
                    sender_id: self.id.into(),
                    msg_tx: self.inner.msg_tx.clone(),
                })
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
