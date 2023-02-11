mod sender_stream;
#[cfg(test)]
mod sender_test;

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use sender_stream::SenderStream;
use tokio::sync::{mpsc, Mutex};
use waitgroup::WaitGroup;

use super::*;
use crate::error::Error;
use crate::*;

pub(crate) struct SenderReportInternal {
    pub(crate) interval: Duration,
    pub(crate) now: Option<FnTimeGen>,
    pub(crate) streams: Mutex<HashMap<u32, Arc<SenderStream>>>,
    pub(crate) close_rx: Mutex<Option<mpsc::Receiver<()>>>,
}

/// SenderReport interceptor generates sender reports.
pub struct SenderReport {
    pub(crate) internal: Arc<SenderReportInternal>,

    pub(crate) wg: Mutex<Option<WaitGroup>>,
    pub(crate) close_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl SenderReport {
    /// builder returns a new ReportBuilder.
    pub fn builder() -> ReportBuilder {
        ReportBuilder {
            is_rr: false,
            ..Default::default()
        }
    }

    async fn is_closed(&self) -> bool {
        let close_tx = self.close_tx.lock().await;
        close_tx.is_none()
    }

    async fn run(
        rtcp_writer: Arc<dyn RTCPWriter + Send + Sync>,
        internal: Arc<SenderReportInternal>,
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

        loop {
            tokio::select! {
                _ = ticker.tick() =>{
                    // TODO(cancel safety): This branch isn't cancel safe
                    let now = if let Some(f) = &internal.now {
                        f()
                    } else {
                        SystemTime::now()
                    };
                    let streams:Vec<Arc<SenderStream>> = {
                        let m = internal.streams.lock().await;
                        m.values().cloned().collect()
                    };
                    for stream in streams {
                        let pkt = stream.generate_report(now).await;

                        let a = Attributes::new();
                        if let Err(err) = rtcp_writer.write(&[Box::new(pkt)], &a).await{
                            log::warn!("failed sending: {}", err);
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
impl Interceptor for SenderReport {
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
            if let Err(err) = SenderReport::run(writer2, internal).await {
                log::warn!("bind_rtcp_writer Generator::run got error: {}", err);
            }
        });

        writer
    }

    /// bind_local_stream lets you modify any outgoing RTP packets. It is called once for per LocalStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        let stream = Arc::new(SenderStream::new(
            info.ssrc,
            info.clock_rate,
            writer,
            self.internal.now.clone(),
        ));
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
