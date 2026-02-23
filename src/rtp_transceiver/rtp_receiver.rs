use crate::error::{Error, Result};
use crate::media_stream::track_remote::TrackRemote;
use crate::peer_connection::{Interceptor, NoopInterceptor, PeerConnectionRef};
use crate::rtp_transceiver::RtpReceiver;
use crate::runtime::Mutex;
use rtc::rtp_transceiver::RTCRtpReceiverId;
use rtc::rtp_transceiver::rtp_receiver::{RTCRtpContributingSource, RTCRtpSynchronizationSource};
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCapabilities, RTCRtpReceiveParameters, RtpCodecKind};
use rtc::statistics::StatsSelector;
use rtc::statistics::report::RTCStatsReport;
use std::sync::Arc;
use std::time::Instant;

/// Concrete async rtp receiver implementation (generic over interceptor type).
///
/// This wraps a rtp receiver and provides async send/receive APIs.
pub(crate) struct RtpReceiverImpl<I = NoopInterceptor>
where
    I: Interceptor,
{
    /// Unique identifier for this rtp receiver
    id: RTCRtpReceiverId,

    /// Inner PeerConnection Reference
    inner: Arc<PeerConnectionRef<I>>,

    track: Mutex<Arc<dyn TrackRemote>>,
}

impl<I> RtpReceiverImpl<I>
where
    I: Interceptor,
{
    /// Create a new rtp receiver wrapper
    pub(crate) fn new(
        id: RTCRtpReceiverId,
        inner: Arc<PeerConnectionRef<I>>,
        track: Arc<dyn TrackRemote>,
    ) -> Self {
        Self {
            id,
            inner,
            track: Mutex::new(track),
        }
    }
}

#[async_trait::async_trait]
impl<I> RtpReceiver for RtpReceiverImpl<I>
where
    I: Interceptor + 'static,
{
    fn id(&self) -> RTCRtpReceiverId {
        self.id
    }

    async fn track(&self) -> Result<Arc<dyn TrackRemote>> {
        {
            let mut peer_connection = self.inner.core.lock().await;

            peer_connection
                .rtp_receiver(self.id)
                .ok_or(Error::ErrRTPReceiverNotExisted)?;
        }

        Ok(self.track.lock().await.clone())
    }

    async fn get_capabilities(&self, kind: RtpCodecKind) -> Result<Option<RTCRtpCapabilities>> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_receiver(self.id)
            .ok_or(Error::ErrRTPReceiverNotExisted)?
            .get_capabilities(kind))
    }

    async fn get_parameters(&self) -> Result<RTCRtpReceiveParameters> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_receiver(self.id)
            .ok_or(Error::ErrRTPReceiverNotExisted)?
            .get_parameters()
            .to_owned())
    }

    async fn get_contributing_sources(&self) -> Result<Vec<RTCRtpContributingSource>> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_receiver(self.id)
            .ok_or(Error::ErrRTPReceiverNotExisted)?
            .get_contributing_sources()
            .map(|s| s.to_owned())
            .collect())
    }

    async fn get_synchronization_sources(&self) -> Result<Vec<RTCRtpSynchronizationSource>> {
        let mut peer_connection = self.inner.core.lock().await;

        Ok(peer_connection
            .rtp_receiver(self.id)
            .ok_or(Error::ErrRTPReceiverNotExisted)?
            .get_synchronization_sources()
            .map(|s| s.to_owned())
            .collect())
    }

    async fn get_stats(&self, now: Instant) -> Result<RTCStatsReport> {
        let mut peer_connection = self.inner.core.lock().await;
        peer_connection
            .rtp_receiver(self.id)
            .ok_or(Error::ErrRTPReceiverNotExisted)?;
        Ok(peer_connection.get_stats(now, StatsSelector::Receiver(self.id)))
    }
}
