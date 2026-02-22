use crate::error::{Error, Result};
use crate::media_stream::TrackLocal;
use crate::peer_connection::{Interceptor, NoopInterceptor, PeerConnectionRef};
use crate::rtp_transceiver::RtpSender;
use rtc::media_stream::MediaStreamId;
use rtc::rtp_transceiver::RTCRtpSenderId;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCapabilities, RTCRtpSendParameters, RTCSetParameterOptions, RtpCodecKind,
};
use rtc::statistics::StatsSelector;
use rtc::statistics::report::RTCStatsReport;
use std::sync::Arc;
use std::time::Instant;

/// Concrete async rtp sender implementation (generic over interceptor type).
///
/// This wraps a rtp sender and provides async send/receive APIs.
pub(crate) struct RtpSenderImpl<I = NoopInterceptor>
where
    I: Interceptor,
{
    /// Unique identifier for this rtp sender
    id: RTCRtpSenderId,

    /// Inner PeerConnection Reference
    inner: Arc<PeerConnectionRef<I>>,
}

impl<I> RtpSenderImpl<I>
where
    I: Interceptor,
{
    /// Create a new rtp sender wrapper
    pub(crate) fn new(id: RTCRtpSenderId, inner: Arc<PeerConnectionRef<I>>) -> Self {
        Self { id, inner }
    }
}

#[async_trait::async_trait]
impl<I> RtpSender for RtpSenderImpl<I>
where
    I: Interceptor + 'static,
{
    async fn track(&self) -> Result<Arc<dyn TrackLocal>> {
        todo!()
    }

    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_sender(self.id)
            .ok_or(Error::ErrRTPSenderNotExisted)?
            .get_capabilities(kind))
    }

    async fn set_parameters(
        &mut self,
        parameters: RTCRtpSendParameters,
        set_parameter_options: Option<RTCSetParameterOptions>,
    ) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_sender(self.id)
            .ok_or(Error::ErrRTPSenderNotExisted)?
            .set_parameters(parameters, set_parameter_options)
    }

    async fn get_parameters(&mut self) -> Result<RTCRtpSendParameters> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_sender(self.id)
            .ok_or(Error::ErrRTPSenderNotExisted)?
            .get_parameters()
            .to_owned())
    }

    async fn replace_track(&mut self, _track: Arc<dyn TrackLocal>) -> Result<()> {
        todo!()
    }

    async fn set_streams(&mut self, streams: Vec<MediaStreamId>) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_sender(self.id)
            .ok_or(Error::ErrRTPSenderNotExisted)?
            .set_streams(streams);
        Ok(())
    }

    async fn get_stats(&self, now: Instant) -> Result<RTCStatsReport> {
        let mut peer_connection = self.inner.core.lock().await;
        peer_connection
            .rtp_sender(self.id)
            .ok_or(Error::ErrRTPSenderNotExisted)?;
        Ok(peer_connection.get_stats(now, StatsSelector::Sender(self.id)))
    }
}
