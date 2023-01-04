#![warn(rust_2018_idioms)]
#![allow(dead_code)]

use async_trait::async_trait;
use error::Result;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use stream_info::StreamInfo;

pub mod chain;
mod error;
pub mod mock;
pub mod nack;
pub mod noop;
pub mod registry;
pub mod report;
pub mod stats;
pub mod stream_info;
pub mod stream_reader;
pub mod twcc;

pub use error::Error;

/// Attributes are a generic key/value store used by interceptors
pub type Attributes = HashMap<usize, usize>;

/// InterceptorBuilder provides an interface for constructing interceptors
pub trait InterceptorBuilder {
    fn build(&self, id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>>;
}

/// Interceptor can be used to add functionality to you PeerConnections by modifying any incoming/outgoing rtp/rtcp
/// packets, or sending your own packets as needed.
#[async_trait]
pub trait Interceptor {
    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync>;

    /// bind_rtcp_writer lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    /// will be called once per packet batch.
    async fn bind_rtcp_writer(
        &self,
        writer: Arc<dyn RTCPWriter + Send + Sync>,
    ) -> Arc<dyn RTCPWriter + Send + Sync>;

    /// bind_local_stream lets you modify any outgoing RTP packets. It is called once for per LocalStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync>;

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, info: &StreamInfo);

    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync>;

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, info: &StreamInfo);

    async fn close(&self) -> Result<()>;
}

/// RTPWriter is used by Interceptor.bind_local_stream.
#[async_trait]
pub trait RTPWriter {
    /// write a rtp packet
    async fn write(&self, pkt: &rtp::packet::Packet, attributes: &Attributes) -> Result<usize>;
}

pub type RTPWriterBoxFn = Box<
    dyn (Fn(
            &rtp::packet::Packet,
            &Attributes,
        ) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + Sync>>)
        + Send
        + Sync,
>;
pub struct RTPWriterFn(pub RTPWriterBoxFn);

#[async_trait]
impl RTPWriter for RTPWriterFn {
    /// write a rtp packet
    async fn write(&self, pkt: &rtp::packet::Packet, attributes: &Attributes) -> Result<usize> {
        self.0(pkt, attributes).await
    }
}

/// RTPReader is used by Interceptor.bind_remote_stream.
#[async_trait]
pub trait RTPReader {
    /// read a rtp packet
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)>;
}

pub type RTPReaderBoxFn = Box<
    dyn (Fn(
            &mut [u8],
            &Attributes,
        ) -> Pin<Box<dyn Future<Output = Result<(usize, Attributes)>> + Send + Sync>>)
        + Send
        + Sync,
>;
pub struct RTPReaderFn(pub RTPReaderBoxFn);

#[async_trait]
impl RTPReader for RTPReaderFn {
    /// read a rtp packet
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)> {
        self.0(buf, attributes).await
    }
}

/// RTCPWriter is used by Interceptor.bind_rtcpwriter.
#[async_trait]
pub trait RTCPWriter {
    /// write a batch of rtcp packets
    async fn write(
        &self,
        pkts: &[Box<dyn rtcp::packet::Packet + Send + Sync>],
        attributes: &Attributes,
    ) -> Result<usize>;
}

pub type RTCPWriterBoxFn = Box<
    dyn (Fn(
            &[Box<dyn rtcp::packet::Packet + Send + Sync>],
            &Attributes,
        ) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + Sync>>)
        + Send
        + Sync,
>;

pub struct RTCPWriterFn(pub RTCPWriterBoxFn);

#[async_trait]
impl RTCPWriter for RTCPWriterFn {
    /// write a batch of rtcp packets
    async fn write(
        &self,
        pkts: &[Box<dyn rtcp::packet::Packet + Send + Sync>],
        attributes: &Attributes,
    ) -> Result<usize> {
        self.0(pkts, attributes).await
    }
}

/// RTCPReader is used by Interceptor.bind_rtcpreader.
#[async_trait]
pub trait RTCPReader {
    /// read a batch of rtcp packets
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)>;
}

pub type RTCPReaderBoxFn = Box<
    dyn (Fn(
            &mut [u8],
            &Attributes,
        ) -> Pin<Box<dyn Future<Output = Result<(usize, Attributes)>> + Send + Sync>>)
        + Send
        + Sync,
>;

pub struct RTCPReaderFn(pub RTCPReaderBoxFn);

#[async_trait]
impl RTCPReader for RTCPReaderFn {
    /// read a batch of rtcp packets
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)> {
        self.0(buf, attributes).await
    }
}

/// Helper for the tests.
#[cfg(test)]
mod test {
    use std::future::Future;
    use std::time::Duration;

    pub async fn timeout_or_fail<T>(duration: Duration, future: T) -> T::Output
    where
        T: Future,
    {
        tokio::time::timeout(duration, future)
            .await
            .expect("should not time out")
    }
}
