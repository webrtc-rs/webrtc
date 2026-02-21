//! Async Media API

use rtc::shared::error::Result;

pub use rtc::rtp_transceiver::{
    RTCRtpReceiverId, RTCRtpSenderId, RTCRtpTransceiverDirection, RTCRtpTransceiverInit,
};

#[async_trait::async_trait]
pub trait RtpReceiver: Send + Sync + 'static {
    async fn close(&self) -> Result<()>;
}

#[async_trait::async_trait]
pub trait RtpSender: Send + Sync + 'static {
    async fn close(&self) -> Result<()>;
}

#[async_trait::async_trait]
pub trait RtpTransceiver: Send + Sync + 'static {
    async fn close(&self) -> Result<()>;
}
