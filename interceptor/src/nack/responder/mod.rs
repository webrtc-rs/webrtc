mod responder_stream;
#[cfg(test)]
mod responder_test;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use responder_stream::ResponderStream;
use rtcp::transport_feedbacks::transport_layer_nack::TransportLayerNack;
use tokio::sync::Mutex;

use crate::error::Result;
use crate::nack::stream_support_nack;
use crate::stream_info::StreamInfo;
use crate::{
    Attributes, Interceptor, InterceptorBuilder, RTCPReader, RTCPWriter, RTPReader, RTPWriter,
};

/// GeneratorBuilder can be used to configure Responder Interceptor
#[derive(Default)]
pub struct ResponderBuilder {
    log2_size: Option<u8>,
}

impl ResponderBuilder {
    /// with_log2_size sets the size of the interceptor.
    /// Size must be one of: 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768
    pub fn with_log2_size(mut self, log2_size: u8) -> ResponderBuilder {
        self.log2_size = Some(log2_size);
        self
    }
}

impl InterceptorBuilder for ResponderBuilder {
    fn build(&self, _id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        Ok(Arc::new(Responder {
            internal: Arc::new(ResponderInternal {
                log2_size: self.log2_size.unwrap_or(13), // 8192 = 1 << 13
                streams: Arc::new(Mutex::new(HashMap::new())),
            }),
        }))
    }
}

pub struct ResponderInternal {
    log2_size: u8,
    streams: Arc<Mutex<HashMap<u32, Arc<ResponderStream>>>>,
}

impl ResponderInternal {
    async fn resend_packets(
        streams: Arc<Mutex<HashMap<u32, Arc<ResponderStream>>>>,
        nack: TransportLayerNack,
    ) {
        let stream = {
            let m = streams.lock().await;
            if let Some(stream) = m.get(&nack.media_ssrc) {
                stream.clone()
            } else {
                return;
            }
        };

        for n in &nack.nacks {
            // can't use n.range() since this callback is async fn,
            // instead, use NackPair into_iter()
            let stream2 = Arc::clone(&stream);
            let f = Box::new(
                move |seq: u16| -> Pin<Box<dyn Future<Output = bool> + Send + 'static>> {
                    let stream3 = Arc::clone(&stream2);
                    Box::pin(async move {
                        if let Some(p) = stream3.get(seq).await {
                            let a = Attributes::new();
                            if let Err(err) = stream3.next_rtp_writer.write(&p, &a).await {
                                log::warn!("failed resending nacked packet: {}", err);
                            }
                        }
                        true
                    })
                },
            );
            for packet_id in n.into_iter() {
                if !f(packet_id).await {
                    return;
                }
            }
        }
    }
}

pub struct ResponderRtcpReader {
    parent_rtcp_reader: Arc<dyn RTCPReader + Send + Sync>,
    internal: Arc<ResponderInternal>,
}

#[async_trait]
impl RTCPReader for ResponderRtcpReader {
    async fn read(
        &self,
        buf: &mut [u8],
        a: &Attributes,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        let (pkts, attr) = { self.parent_rtcp_reader.read(buf, a).await? };
        for p in &pkts {
            if let Some(nack) = p.as_any().downcast_ref::<TransportLayerNack>() {
                let nack = nack.clone();
                let streams = Arc::clone(&self.internal.streams);
                tokio::spawn(async move {
                    ResponderInternal::resend_packets(streams, nack).await;
                });
            }
        }

        Ok((pkts, attr))
    }
}

/// Responder responds to nack feedback messages
pub struct Responder {
    internal: Arc<ResponderInternal>,
}

impl Responder {
    /// builder returns a new ResponderBuilder.
    pub fn builder() -> ResponderBuilder {
        ResponderBuilder::default()
    }
}

#[async_trait]
impl Interceptor for Responder {
    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        Arc::new(ResponderRtcpReader {
            internal: Arc::clone(&self.internal),
            parent_rtcp_reader: reader,
        }) as Arc<dyn RTCPReader + Send + Sync>
    }

    /// bind_rtcp_writer lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    /// will be called once per packet batch.
    async fn bind_rtcp_writer(
        &self,
        writer: Arc<dyn RTCPWriter + Send + Sync>,
    ) -> Arc<dyn RTCPWriter + Send + Sync> {
        writer
    }

    /// bind_local_stream lets you modify any outgoing RTP packets. It is called once for per LocalStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        if !stream_support_nack(info) {
            return writer;
        }

        let stream = Arc::new(ResponderStream::new(self.internal.log2_size, writer));
        {
            let mut streams = self.internal.streams.lock().await;
            streams.insert(info.ssrc, Arc::clone(&stream));
        }

        stream
    }

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, info: &StreamInfo) {
        let mut streams = self.internal.streams.lock().await;
        streams.remove(&info.ssrc);
    }

    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        _info: &StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        reader
    }

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, _info: &StreamInfo) {}

    /// close closes the Interceptor, cleaning up any data if necessary.
    async fn close(&self) -> Result<()> {
        Ok(())
    }
}
