//! Async Media API

use crate::media_stream::{TrackLocal, TrackRemote};
use rtc::media_stream::MediaStreamId;
use rtc::rtp_transceiver::rtp_receiver::{RTCRtpContributingSource, RTCRtpSynchronizationSource};
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCapabilities, RTCRtpCodec, RTCRtpReceiveParameters, RTCRtpSendParameters,
    RTCSetParameterOptions, RtpCodecKind,
};
pub use rtc::rtp_transceiver::{
    RTCRtpReceiverId, RTCRtpSenderId, RTCRtpTransceiverDirection, RTCRtpTransceiverInit,
};
use rtc::shared::error::Result;
use rtc::statistics::StatsSelector;
use rtc::statistics::report::RTCStatsReport;
use std::sync::Arc;
use std::time::Instant;

#[async_trait::async_trait]
pub trait RtpReceiver: Send + Sync + 'static {
    async fn track(&self) -> Arc<dyn TrackRemote>;
    async fn get_capabilities(&self, kind: RtpCodecKind) -> Option<RTCRtpCapabilities>;
    async fn get_parameters(&mut self) -> RTCRtpReceiveParameters;
    async fn get_contributing_sources(&self) -> Vec<RTCRtpContributingSource>;
    async fn get_synchronization_sources(&self) -> Vec<RTCRtpSynchronizationSource>;
    async fn get_stats(&self, now: Instant, selector: StatsSelector) -> RTCStatsReport;
}

#[async_trait::async_trait]
pub trait RtpSender: Send + Sync + 'static {
    async fn track(&self) -> Arc<dyn TrackLocal>;
    async fn get_capabilities(&self, kind: RtpCodecKind) -> Option<RTCRtpCapabilities>;
    async fn set_parameters(
        &mut self,
        parameters: RTCRtpSendParameters,
        set_parameter_options: Option<RTCSetParameterOptions>,
    ) -> Result<()>;
    async fn get_parameters(&mut self) -> RTCRtpSendParameters;
    async fn replace_track(&mut self, track: Arc<dyn TrackLocal>) -> Result<()>;
    async fn set_streams(&mut self, streams: Vec<MediaStreamId>);
    async fn get_stats(&self, now: Instant, selector: StatsSelector) -> RTCStatsReport;
}

#[async_trait::async_trait]
pub trait RtpTransceiver: Send + Sync + 'static {
    async fn mid(&self) -> Option<String>;
    async fn sender(&self) -> Option<Arc<dyn RtpSender>>;
    async fn receiver(&self) -> Option<Arc<dyn RtpReceiver>>;
    async fn direction(&self) -> RTCRtpTransceiverDirection;
    async fn current_direction(&self) -> RTCRtpTransceiverDirection;
    async fn stop(&self) -> Result<()>;
    async fn set_codec_preferences(&self, codecs: Vec<RTCRtpCodec>) -> Result<()>;
}
