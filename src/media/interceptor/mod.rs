pub mod stream_info;

use stream_info::StreamInfo;

use crate::media::track::track_local::TrackLocalWriter;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;

/// Interceptor can be used to add functionality to you PeerConnections by modifying any incoming/outgoing rtp/rtcp
/// packets, or sending your own packets as needed.
#[async_trait]
pub trait Interceptor {
    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(&self, reader: Box<dyn RTCPReader>) -> Box<dyn RTCPReader>;

    /// bind_rtcp_writer lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    /// will be called once per packet batch.
    async fn bind_rtcp_writer(&self, writer: Box<dyn RTCPWriter>) -> Box<dyn RTCPWriter>;

    /// bind_local_stream lets you modify any outgoing RTP packets. It is called once for per LocalStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        writer: Box<dyn RTPWriter>,
    ) -> Box<dyn RTPWriter>;

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, info: &StreamInfo);

    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        reader: Box<dyn RTPReader>,
    ) -> Box<dyn RTPReader>;

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, info: &StreamInfo);

    async fn close(&self) -> Result<()>;
}

/// RTPWriter is used by Interceptor.bind_local_stream.
#[async_trait]
pub trait RTPWriter {
    /// write a rtp packet
    async fn write(
        &self,
        header: &rtp::header::Header,
        payload: &Bytes,
        attributes: &Attributes,
    ) -> Result<usize>;
}

/// RTPReader is used by Interceptor.bind_remote_stream.
#[async_trait]
pub trait RTPReader {
    /// read a rtp packet
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)>;
}

/// RTCPWriter is used by Interceptor.bind_rtcpwriter.
#[async_trait]
pub trait RTCPWriter {
    /// write a batch of rtcp packets
    async fn write(
        &self,
        pkts: &dyn rtcp::packet::Packet,
        attributes: &Attributes,
    ) -> Result<usize>;
}

/// RTCPReader is used by Interceptor.bind_rtcpreader.
#[async_trait]
pub trait RTCPReader {
    /// read a batch of rtcp packets
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)>;
}

/// Attributes are a generic key/value store used by interceptors
pub type Attributes = HashMap<usize, usize>;
/*
/// RTPWriterFunc is an adapter for RTPWrite interface
type RTPWriterFunc func(header *rtp.Header, payload []byte, attributes Attributes) (int, error)

/// RTPReaderFunc is an adapter for RTPReader interface
type RTPReaderFunc func([]byte, Attributes) (int, Attributes, error)

/// RTCPWriterFunc is an adapter for RTCPWriter interface
type RTCPWriterFunc func(pkts []rtcp.Packet, attributes Attributes) (int, error)

/// RTCPReaderFunc is an adapter for RTCPReader interface
type RTCPReaderFunc func([]byte, Attributes) (int, Attributes, error)

/// Write a rtp packet
func (f RTPWriterFunc) Write(header *rtp.Header, payload []byte, attributes Attributes) (int, error) {
    return f(header, payload, attributes)
}

/// Read a rtp packet
func (f RTPReaderFunc) Read(b []byte, a Attributes) (int, Attributes, error) {
    return f(b, a)
}

/// Write a batch of rtcp packets
func (f RTCPWriterFunc) Write(pkts []rtcp.Packet, attributes Attributes) (int, error) {
    return f(pkts, attributes)
}

/// Read a batch of rtcp packets
func (f RTCPReaderFunc) Read(b []byte, a Attributes) (int, Attributes, error) {
    return f(b, a)
}

/// Get returns the attribute associated with key.
func (a Attributes) Get(key interface{}) interface{} {
    return a[key]
}

/// Set sets the attribute associated with key to the given value.
func (a Attributes) Set(key interface{}, val interface{}) {
    a[key] = val
}
*/

#[derive(Debug, Clone)]
pub(crate) struct InterceptorToTrackLocalWriter {
    // interceptor atomic.Value //  // interceptor.RTPWriter }
}

#[async_trait]
impl TrackLocalWriter for InterceptorToTrackLocalWriter {
    async fn write_rtp(&self, _p: &rtp::packet::Packet) -> Result<usize> {
        /*TODO:
           if writer, ok := i.interceptor.Load().(interceptor.RTPWriter); ok && writer != nil {
            return writer.Write(header, payload, interceptor.Attributes{})
        }

        return 0, nil*/
        Ok(0)
    }

    async fn write(&self, _b: &Bytes) -> Result<usize> {
        /*TODO:
           packet := &rtp.Packet{}
        if err := packet.Unmarshal(b); err != nil {
            return 0, err
        }

        return i.WriteRTP(&packet.Header, packet.Payload)*/
        Ok(0)
    }

    fn clone_to(&self) -> Box<dyn TrackLocalWriter + Send + Sync> {
        Box::new(self.clone())
    }
}
