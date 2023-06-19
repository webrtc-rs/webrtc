use std::sync::Arc;

use crate::error::*;
use crate::stream_info::StreamInfo;
use crate::*;

/// Chain is an interceptor that runs all child interceptors in order.
#[derive(Default)]
pub struct Chain {
    interceptors: Vec<Arc<dyn Interceptor + Send + Sync>>,
}

impl Chain {
    /// new returns a new Chain interceptor.
    pub fn new(interceptors: Vec<Arc<dyn Interceptor + Send + Sync>>) -> Self {
        Chain { interceptors }
    }

    pub fn add(&mut self, icpr: Arc<dyn Interceptor + Send + Sync>) {
        self.interceptors.push(icpr);
    }
}

#[async_trait]
impl Interceptor for Chain {
    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(
        &self,
        mut reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        for icpr in &self.interceptors {
            reader = icpr.bind_rtcp_reader(reader).await;
        }
        reader
    }

    /// bind_rtcp_writer lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    /// will be called once per packet batch.
    async fn bind_rtcp_writer(
        &self,
        mut writer: Arc<dyn RTCPWriter + Send + Sync>,
    ) -> Arc<dyn RTCPWriter + Send + Sync> {
        for icpr in &self.interceptors {
            writer = icpr.bind_rtcp_writer(writer).await;
        }
        writer
    }

    /// bind_local_stream lets you modify any outgoing RTP packets. It is called once for per LocalStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        mut writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        for icpr in &self.interceptors {
            writer = icpr.bind_local_stream(info, writer).await;
        }
        writer
    }

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, info: &StreamInfo) {
        for icpr in &self.interceptors {
            icpr.unbind_local_stream(info).await;
        }
    }

    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        mut reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        for icpr in &self.interceptors {
            reader = icpr.bind_remote_stream(info, reader).await;
        }
        reader
    }

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, info: &StreamInfo) {
        for icpr in &self.interceptors {
            icpr.unbind_remote_stream(info).await;
        }
    }

    /// close closes the Interceptor, cleaning up any data if necessary.
    async fn close(&self) -> Result<()> {
        let mut errs = vec![];
        for icpr in &self.interceptors {
            if let Err(err) = icpr.close().await {
                errs.push(err);
            }
        }
        flatten_errs(errs)
    }
}
