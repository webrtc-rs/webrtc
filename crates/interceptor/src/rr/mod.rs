pub mod receiver_stream;

use crate::*;
use receiver_stream::ReceiverStream;

use anyhow::Result;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, Mutex};

pub type NowFn = Box<dyn (Fn() -> SystemTime) + Send + Sync>;

/// ReceiverBuilder can be used to configure ReceiverReport Interceptor.
#[derive(Default)]
pub struct ReceiverBuilder {
    interval: Option<Duration>,
    now: Option<NowFn>,
}

impl ReceiverBuilder {
    /// with_interval sets send interval for the interceptor.
    pub fn with_interval(mut self, interval: Duration) -> ReceiverBuilder {
        self.interval = Some(interval);
        self
    }

    /// ReceiverNow sets an alternative for the time.Now function.
    pub fn with_now_fn(mut self, now: NowFn) -> ReceiverBuilder {
        self.now = Some(now);
        self
    }

    pub fn build(mut self) -> ReceiverReport {
        let (close_tx, close_rx) = mpsc::channel(1);
        ReceiverReport {
            interval: if let Some(interval) = self.interval.take() {
                interval
            } else {
                Duration::from_secs(1)
            },
            now: self.now.take(),

            streams: Mutex::new(HashMap::new()),
            close_rx,
            close_tx: Mutex::new(Some(close_tx)),
        }
    }
}

/// ReceiverReport interceptor generates receiver reports.
pub struct ReceiverReport {
    interval: Duration,
    now: Option<NowFn>,

    streams: Mutex<HashMap<u32, Arc<ReceiverStream>>>,
    //wg       sync.WaitGroup
    close_rx: mpsc::Receiver<()>,
    close_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl ReceiverReport {
    /// builder returns a new ReceiverReport builder.
    pub fn builder() -> ReceiverBuilder {
        ReceiverBuilder::default()
    }

    /*

    func (r *ReceiverInterceptor) loop(rtcpWriter interceptor.RTCPWriter) {
        defer r.wg.Done()

        ticker := time.NewTicker(r.interval)
        defer ticker.Stop()
        for {
            select {
            case <-ticker.C:
                now := r.now()
                r.streams.Range(func(key, value interface{}) bool {
                    stream := value.(*receiverStream)

                    var pkts []rtcp.Packet

                    pkts = append(pkts, stream.generateReport(now))

                    if _, err := rtcpWriter.Write(pkts, interceptor.Attributes{}); err != nil {
                        r.log.Warnf("failed sending: %+v", err)
                    }

                    return true
                })

            case <-r.close:
                return
            }
        }
    }
     */
}

#[async_trait]
impl Interceptor for ReceiverReport {
    /*

    // BindRTCPReader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    // change in the future. The returned method will be called once per packet batch.
    func (r *ReceiverInterceptor) BindRTCPReader(reader interceptor.RTCPReader) interceptor.RTCPReader {
        return interceptor.RTCPReaderFunc(func(b []byte, a interceptor.Attributes) (int, interceptor.Attributes, error) {
            i, attr, err := reader.Read(b, a)
            if err != nil {
                return 0, nil, err
            }

            pkts, err := rtcp.Unmarshal(b[:i])
            if err != nil {
                return 0, nil, err
            }

            for _, pkt := range pkts {
                if sr, ok := (pkt).(*rtcp.SenderReport); ok {
                    value, ok := r.streams.Load(sr.SSRC)
                    if !ok {
                        continue
                    }

                    stream := value.(*receiverStream)
                    stream.processSenderReport(r.now(), sr)
                }
            }

            return i, attr, nil
        })
       }

    // BindRTCPWriter lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    // will be called once per packet batch.
    func (r *ReceiverInterceptor) BindRTCPWriter(writer interceptor.RTCPWriter) interceptor.RTCPWriter {
        r.m.Lock()
        defer r.m.Unlock()

        if r.isClosed() {
            return writer
        }

        r.wg.Add(1)

        go r.loop(writer)

        return writer
    }



    */
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
    async fn unbind_local_stream(&self, info: &StreamInfo) {
        let mut streams = self.streams.lock().await;
        streams.remove(&info.ssrc);
    }

    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        let stream = Arc::new(ReceiverStream::new(info.ssrc, info.clock_rate, reader));
        {
            let mut streams = self.streams.lock().await;
            streams.insert(info.ssrc, Arc::clone(&stream));
        }

        stream
    }

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, _info: &StreamInfo) {}

    /// close closes the interceptor.
    async fn close(&self) -> Result<()> {
        let mut close_tx = self.close_tx.lock().await;
        close_tx.take();

        Ok(())
    }
}
