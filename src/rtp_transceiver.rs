use crate::Result;

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
