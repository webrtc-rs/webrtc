use crate::error::{Error, Result};
use crate::media_stream::track_local::TrackLocal;
use crate::peer_connection::PeerConnectionRef;
use crate::peer_connection::transport::{DtlsRoute, DtlsTransport, DtlsTransportImpl};
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
pub(crate) struct RtpSenderImpl {
    /// Unique identifier for this rtp sender
    id: RTCRtpSenderId,

    /// Inner PeerConnection Reference
    inner: Arc<PeerConnectionRef>,

    track: Arc<dyn TrackLocal>,
}

impl RtpSenderImpl {
    /// Create a new rtp sender wrapper
    pub(crate) fn new(
        id: RTCRtpSenderId,
        inner: Arc<PeerConnectionRef>,
        track: Arc<dyn TrackLocal>,
    ) -> Self {
        Self { id, inner, track }
    }
}

impl crate::sealed::Sealed for RtpSenderImpl {}

#[async_trait::async_trait]
impl RtpSender for RtpSenderImpl {
    fn id(&self) -> RTCRtpSenderId {
        self.id
    }

    fn track(&self) -> &Arc<dyn TrackLocal> {
        &self.track
    }

    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_sender(self.id)
            .ok_or(Error::ErrRTPSenderNotExisted)?
            .get_capabilities(kind))
    }

    async fn set_parameters(
        &self,
        parameters: RTCRtpSendParameters,
        set_parameter_options: Option<RTCSetParameterOptions>,
    ) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_sender(self.id)
            .ok_or(Error::ErrRTPSenderNotExisted)?
            .set_parameters(parameters, set_parameter_options)
    }

    async fn get_parameters(&self) -> Result<RTCRtpSendParameters> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_sender(self.id)
            .ok_or(Error::ErrRTPSenderNotExisted)?
            .get_parameters()
            .to_owned())
    }

    async fn replace_track(&self, track: Arc<dyn TrackLocal>) -> Result<()> {
        let mut peer_connection = self.inner.core.lock().await;

        peer_connection
            .rtp_sender(self.id)
            .ok_or(Error::ErrRTPSenderNotExisted)?
            .replace_track(track.track().await)
    }

    async fn set_streams(&self, streams: Vec<MediaStreamId>) -> Result<()> {
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
    async fn transport(&self) -> Result<Option<Arc<dyn DtlsTransport>>> {
        let mut peer_connection = self.inner.core.lock().await;

        // Walk under the lock and keep only the ids: a borrowed view cannot cross an await, so
        // the handle re-walks per call and carries the ids so `id()` can stay synchronous.
        let ids = match peer_connection.rtp_sender(self.id) {
            Some(sender) => sender
                .transport()
                .map(|dtls| (dtls.id(), dtls.ice_transport().id())),
            None => return Err(Error::ErrRTPSenderNotExisted),
        };
        drop(peer_connection);

        Ok(ids.map(|(id, ice_id)| {
            Arc::new(DtlsTransportImpl::new(
                id,
                ice_id,
                DtlsRoute::Sender(self.id),
                Arc::clone(&self.inner),
            )) as Arc<dyn DtlsTransport>
        }))
    }
}
