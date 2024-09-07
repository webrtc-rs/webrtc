mod generator_stream;
#[cfg(test)]
mod generator_test;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use generator_stream::GeneratorStream;
use rtcp::transport_feedbacks::transport_layer_nack::{
    nack_pairs_from_sequence_numbers, TransportLayerNack,
};
use tokio::sync::{mpsc, Mutex};
use waitgroup::WaitGroup;

use crate::error::{Error, Result};
use crate::nack::stream_support_nack;
use crate::stream_info::StreamInfo;
use crate::{
    Attributes, Interceptor, InterceptorBuilder, RTCPReader, RTCPWriter, RTPReader, RTPWriter,
};

/// GeneratorBuilder can be used to configure Generator Interceptor
#[derive(Default)]
pub struct GeneratorBuilder {
    log2_size_minus_6: Option<u8>,
    skip_last_n: Option<u16>,
    interval: Option<Duration>,
}

impl GeneratorBuilder {
    /// with_size sets the size of the interceptor.
    /// Size must be one of: 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768
    pub fn with_log2_size_minus_6(mut self, log2_size_minus_6: u8) -> GeneratorBuilder {
        self.log2_size_minus_6 = Some(log2_size_minus_6);
        self
    }

    /// with_skip_last_n sets the number of packets (n-1 packets before the last received packets) to ignore when generating
    /// nack requests.
    pub fn with_skip_last_n(mut self, skip_last_n: u16) -> GeneratorBuilder {
        self.skip_last_n = Some(skip_last_n);
        self
    }

    /// with_interval sets the nack send interval for the interceptor
    pub fn with_interval(mut self, interval: Duration) -> GeneratorBuilder {
        self.interval = Some(interval);
        self
    }
}

impl InterceptorBuilder for GeneratorBuilder {
    fn build(&self, _id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        let (close_tx, close_rx) = mpsc::channel(1);
        Ok(Arc::new(Generator {
            internal: Arc::new(GeneratorInternal {
                log2_size_minus_6: self.log2_size_minus_6.unwrap_or(13 - 6), // 8192 = 1 << 13
                skip_last_n: self.skip_last_n.unwrap_or_default(),
                interval: if let Some(interval) = self.interval {
                    interval
                } else {
                    Duration::from_millis(100)
                },

                streams: Mutex::new(HashMap::new()),
                close_rx: Mutex::new(Some(close_rx)),
            }),

            wg: Mutex::new(Some(WaitGroup::new())),
            close_tx: Mutex::new(Some(close_tx)),
        }))
    }
}

struct GeneratorInternal {
    log2_size_minus_6: u8,
    skip_last_n: u16,
    interval: Duration,

    streams: Mutex<HashMap<u32, Arc<GeneratorStream>>>,
    close_rx: Mutex<Option<mpsc::Receiver<()>>>,
}

/// Generator interceptor generates nack feedback messages.
pub struct Generator {
    internal: Arc<GeneratorInternal>,

    pub(crate) wg: Mutex<Option<WaitGroup>>,
    pub(crate) close_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl Generator {
    /// builder returns a new GeneratorBuilder.
    pub fn builder() -> GeneratorBuilder {
        GeneratorBuilder::default()
    }

    async fn is_closed(&self) -> bool {
        let close_tx = self.close_tx.lock().await;
        close_tx.is_none()
    }

    async fn run(
        rtcp_writer: Arc<dyn RTCPWriter + Send + Sync>,
        internal: Arc<GeneratorInternal>,
    ) -> Result<()> {
        let mut ticker = tokio::time::interval(internal.interval);
        let mut close_rx = {
            let mut close_rx = internal.close_rx.lock().await;
            if let Some(close) = close_rx.take() {
                close
            } else {
                return Err(Error::ErrInvalidCloseRx);
            }
        };

        let sender_ssrc = rand::random::<u32>();
        loop {
            tokio::select! {
                _ = ticker.tick() =>{
                    let nacks = {
                        let mut nacks = vec![];
                        let streams = internal.streams.lock().await;
                        for (ssrc, stream) in streams.iter() {
                            let missing = stream.missing_seq_numbers(internal.skip_last_n);
                            if missing.is_empty(){
                                continue;
                            }

                            nacks.push(TransportLayerNack{
                                sender_ssrc,
                                media_ssrc: *ssrc,
                                nacks:  nack_pairs_from_sequence_numbers(&missing),
                            });
                        }
                        nacks
                    };

                    let a = Attributes::new();
                    for nack in nacks{
                        if let Err(err) = rtcp_writer.write(&[Box::new(nack)], &a).await{
                            log::warn!("failed sending nack: {}", err);
                        }
                    }
                }
                _ = close_rx.recv() =>{
                    return Ok(());
                }
            }
        }
    }
}

#[async_trait]
impl Interceptor for Generator {
    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        reader
    }

    /// bind_rtcp_writer lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    /// will be called once per packet batch.
    async fn bind_rtcp_writer(
        &self,
        writer: Arc<dyn RTCPWriter + Send + Sync>,
    ) -> Arc<dyn RTCPWriter + Send + Sync> {
        if self.is_closed().await {
            return writer;
        }

        let mut w = {
            let wait_group = self.wg.lock().await;
            wait_group.as_ref().map(|wg| wg.worker())
        };
        let writer2 = Arc::clone(&writer);
        let internal = Arc::clone(&self.internal);
        tokio::spawn(async move {
            let _d = w.take();
            if let Err(err) = Generator::run(writer2, internal).await {
                log::warn!("bind_rtcp_writer NACK Generator::run got error: {}", err);
            }
        });

        writer
    }

    /// bind_local_stream lets you modify any outgoing RTP packets. It is called once for per LocalStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_local_stream(
        &self,
        _info: &StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        writer
    }

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, _info: &StreamInfo) {}

    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        if !stream_support_nack(info) {
            return reader;
        }

        let stream = Arc::new(GeneratorStream::new(
            self.internal.log2_size_minus_6,
            reader,
        ));
        {
            let mut streams = self.internal.streams.lock().await;
            streams.insert(info.ssrc, Arc::clone(&stream));
        }

        stream
    }

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, info: &StreamInfo) {
        let mut receive_logs = self.internal.streams.lock().await;
        receive_logs.remove(&info.ssrc);
    }

    /// close closes the Interceptor, cleaning up any data if necessary.
    async fn close(&self) -> Result<()> {
        {
            let mut close_tx = self.close_tx.lock().await;
            close_tx.take();
        }

        {
            let mut wait_group = self.wg.lock().await;
            if let Some(wg) = wait_group.take() {
                wg.wait().await;
            }
        }

        Ok(())
    }
}
