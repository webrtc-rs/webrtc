mod receiver_stream;
#[cfg(test)]
mod receiver_test;

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use receiver_stream::ReceiverStream;
use tokio::sync::{mpsc, Mutex};
use waitgroup::WaitGroup;

use super::*;
use crate::error::Error;
use crate::*;

pub(crate) struct ReceiverReportInternal {
    pub(crate) interval: Duration,
    pub(crate) now: Option<FnTimeGen>,
    pub(crate) streams: Mutex<HashMap<u32, Arc<ReceiverStream>>>,
    pub(crate) close_rx: Mutex<Option<mpsc::Receiver<()>>>,
}

pub(crate) struct ReceiverReportRtcpReader {
    pub(crate) internal: Arc<ReceiverReportInternal>,
    pub(crate) parent_rtcp_reader: Arc<dyn RTCPReader + Send + Sync>,
}

#[async_trait]
impl RTCPReader for ReceiverReportRtcpReader {
    async fn read(
        &self,
        buf: &mut [u8],
        a: &Attributes,
    ) -> Result<(Vec<Box<dyn rtcp::packet::Packet + Send + Sync>>, Attributes)> {
        let (pkts, attr) = self.parent_rtcp_reader.read(buf, a).await?;

        let now = if let Some(f) = &self.internal.now {
            f()
        } else {
            SystemTime::now()
        };

        for p in &pkts {
            if let Some(sr) = p
                .as_any()
                .downcast_ref::<rtcp::sender_report::SenderReport>()
            {
                let stream = {
                    let m = self.internal.streams.lock().await;
                    m.get(&sr.ssrc).cloned()
                };
                if let Some(stream) = stream {
                    stream.process_sender_report(now, sr);
                }
            }
        }

        Ok((pkts, attr))
    }
}

/// ReceiverReport interceptor generates receiver reports.
pub struct ReceiverReport {
    pub(crate) internal: Arc<ReceiverReportInternal>,

    pub(crate) wg: Mutex<Option<WaitGroup>>,
    pub(crate) close_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl ReceiverReport {
    /// builder returns a new ReportBuilder.
    pub fn builder() -> ReportBuilder {
        ReportBuilder {
            is_rr: true,
            ..Default::default()
        }
    }

    async fn is_closed(&self) -> bool {
        let close_tx = self.close_tx.lock().await;
        close_tx.is_none()
    }

    async fn run(
        rtcp_writer: Arc<dyn RTCPWriter + Send + Sync>,
        internal: Arc<ReceiverReportInternal>,
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
                    let streams:Vec<Arc<ReceiverStream>> = {
                        let m = internal.streams.lock().await;
                        m.values().cloned().collect()
                    };
                    for stream in streams {
                        let pkt = stream.generate_report(now);

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
impl Interceptor for ReceiverReport {
    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        Arc::new(ReceiverReportRtcpReader {
            internal: Arc::clone(&self.internal),
            parent_rtcp_reader: reader,
        })
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
            if let Err(err) = ReceiverReport::run(writer2, internal).await {
                log::warn!("bind_rtcp_writer ReceiverReport::run got error: {}", err);
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

    /// UnbindLocalStream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, _info: &StreamInfo) {}

    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        let stream = Arc::new(ReceiverStream::new(
            info.ssrc,
            info.clock_rate,
            reader,
            self.internal.now.clone(),
        ));
        {
            let mut streams = self.internal.streams.lock().await;
            streams.insert(info.ssrc, Arc::clone(&stream));
        }

        stream
    }

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, info: &StreamInfo) {
        let mut streams = self.internal.streams.lock().await;
        streams.remove(&info.ssrc);
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
