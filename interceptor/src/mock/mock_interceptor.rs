use std::future::Future;
use std::pin::Pin;

use crate::*;

pub type BindRtcpReaderFn = Box<
    dyn (Fn(
            Arc<dyn RTCPReader + Send + Sync>,
        )
            -> Pin<Box<dyn Future<Output = Arc<dyn RTCPReader + Send + Sync>> + Send + Sync>>)
        + Send
        + Sync,
>;

pub type BindRtcpWriterFn = Box<
    dyn (Fn(
            Arc<dyn RTCPWriter + Send + Sync>,
        )
            -> Pin<Box<dyn Future<Output = Arc<dyn RTCPWriter + Send + Sync>> + Send + Sync>>)
        + Send
        + Sync,
>;
pub type BindLocalStreamFn = Box<
    dyn (Fn(
            &StreamInfo,
            Arc<dyn RTPWriter + Send + Sync>,
        ) -> Pin<Box<dyn Future<Output = Arc<dyn RTPWriter + Send + Sync>> + Send + Sync>>)
        + Send
        + Sync,
>;
pub type UnbindLocalStreamFn =
    Box<dyn (Fn(&StreamInfo) -> Pin<Box<dyn Future<Output = ()> + Send + Sync>>) + Send + Sync>;
pub type BindRemoteStreamFn = Box<
    dyn (Fn(
            &StreamInfo,
            Arc<dyn RTPReader + Send + Sync>,
        ) -> Pin<Box<dyn Future<Output = Arc<dyn RTPReader + Send + Sync>> + Send + Sync>>)
        + Send
        + Sync,
>;
pub type UnbindRemoteStreamFn =
    Box<dyn (Fn(&StreamInfo) -> Pin<Box<dyn Future<Output = ()> + Send + Sync>>) + Send + Sync>;
pub type CloseFn =
    Box<dyn (Fn() -> Pin<Box<dyn Future<Output = Result<()>> + Send + Sync>>) + Send + Sync>;

/// MockInterceptor is an mock Interceptor for testing.
#[derive(Default)]
pub struct MockInterceptor {
    pub bind_rtcp_reader_fn: Option<BindRtcpReaderFn>,
    pub bind_rtcp_writer_fn: Option<BindRtcpWriterFn>,
    pub bind_local_stream_fn: Option<BindLocalStreamFn>,
    pub unbind_local_stream_fn: Option<UnbindLocalStreamFn>,
    pub bind_remote_stream_fn: Option<BindRemoteStreamFn>,
    pub unbind_remote_stream_fn: Option<UnbindRemoteStreamFn>,
    pub close_fn: Option<CloseFn>,
}

#[async_trait]
impl Interceptor for MockInterceptor {
    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        if let Some(f) = &self.bind_rtcp_reader_fn {
            f(reader).await
        } else {
            reader
        }
    }

    /// bind_rtcp_writer lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    /// will be called once per packet batch.
    async fn bind_rtcp_writer(
        &self,
        writer: Arc<dyn RTCPWriter + Send + Sync>,
    ) -> Arc<dyn RTCPWriter + Send + Sync> {
        if let Some(f) = &self.bind_rtcp_writer_fn {
            f(writer).await
        } else {
            writer
        }
    }

    /// bind_local_stream lets you modify any outgoing RTP packets. It is called once for per LocalStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        if let Some(f) = &self.bind_local_stream_fn {
            f(info, writer).await
        } else {
            writer
        }
    }

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, info: &StreamInfo) {
        if let Some(f) = &self.unbind_local_stream_fn {
            f(info).await
        }
    }

    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        if let Some(f) = &self.bind_remote_stream_fn {
            f(info, reader).await
        } else {
            reader
        }
    }

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, info: &StreamInfo) {
        if let Some(f) = &self.unbind_remote_stream_fn {
            f(info).await
        }
    }

    /// close closes the Interceptor, cleaning up any data if necessary.
    async fn close(&self) -> Result<()> {
        if let Some(f) = &self.close_fn {
            f().await
        } else {
            Ok(())
        }
    }
}
