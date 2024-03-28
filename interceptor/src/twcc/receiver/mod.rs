mod receiver_stream;
#[cfg(test)]
mod receiver_test;

use std::time::Duration;

use receiver_stream::ReceiverStream;
use rtp::extension::transport_cc_extension::TransportCcExtension;
use tokio::sync::{mpsc, Mutex};
use tokio::time::MissedTickBehavior;
use util::Unmarshal;
use waitgroup::WaitGroup;

use crate::twcc::sender::TRANSPORT_CC_URI;
use crate::twcc::Recorder;
use crate::*;

/// ReceiverBuilder is a InterceptorBuilder for a SenderInterceptor
#[derive(Default)]
pub struct ReceiverBuilder {
    interval: Option<Duration>,
}

impl ReceiverBuilder {
    /// with_interval sets send interval for the interceptor.
    pub fn with_interval(mut self, interval: Duration) -> ReceiverBuilder {
        self.interval = Some(interval);
        self
    }
}

impl InterceptorBuilder for ReceiverBuilder {
    fn build(&self, _id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        let (close_tx, close_rx) = mpsc::channel(1);
        let (packet_chan_tx, packet_chan_rx) = mpsc::channel(1);
        Ok(Arc::new(Receiver {
            internal: Arc::new(ReceiverInternal {
                interval: if let Some(interval) = &self.interval {
                    *interval
                } else {
                    Duration::from_millis(100)
                },
                recorder: Mutex::new(Recorder::default()),
                packet_chan_rx: Mutex::new(Some(packet_chan_rx)),
                streams: Mutex::new(HashMap::new()),
                close_rx: Mutex::new(Some(close_rx)),
            }),
            start_time: tokio::time::Instant::now(),
            packet_chan_tx,
            wg: Mutex::new(Some(WaitGroup::new())),
            close_tx: Mutex::new(Some(close_tx)),
        }))
    }
}

struct Packet {
    hdr: rtp::header::Header,
    sequence_number: u16,
    arrival_time: i64,
    ssrc: u32,
}

struct ReceiverInternal {
    interval: Duration,
    recorder: Mutex<Recorder>,
    packet_chan_rx: Mutex<Option<mpsc::Receiver<Packet>>>,
    streams: Mutex<HashMap<u32, Arc<ReceiverStream>>>,
    close_rx: Mutex<Option<mpsc::Receiver<()>>>,
}

/// Receiver sends transport-wide congestion control reports as specified in:
/// <https://datatracker.ietf.org/doc/html/draft-holmer-rmcat-transport-wide-cc-extensions-01>
pub struct Receiver {
    internal: Arc<ReceiverInternal>,

    // we use tokio's Instant because it makes testing easier via `tokio::time::advance`.
    start_time: tokio::time::Instant,
    packet_chan_tx: mpsc::Sender<Packet>,

    wg: Mutex<Option<WaitGroup>>,
    close_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl Receiver {
    /// builder returns a new ReceiverBuilder.
    pub fn builder() -> ReceiverBuilder {
        ReceiverBuilder::default()
    }

    async fn is_closed(&self) -> bool {
        let close_tx = self.close_tx.lock().await;
        close_tx.is_none()
    }

    async fn run(
        rtcp_writer: Arc<dyn RTCPWriter + Send + Sync>,
        internal: Arc<ReceiverInternal>,
    ) -> Result<()> {
        let mut close_rx = {
            let mut close_rx = internal.close_rx.lock().await;
            if let Some(close_rx) = close_rx.take() {
                close_rx
            } else {
                return Err(Error::ErrInvalidCloseRx);
            }
        };
        let mut packet_chan_rx = {
            let mut packet_chan_rx = internal.packet_chan_rx.lock().await;
            if let Some(packet_chan_rx) = packet_chan_rx.take() {
                packet_chan_rx
            } else {
                return Err(Error::ErrInvalidPacketRx);
            }
        };

        let a = Attributes::new();
        let mut ticker = tokio::time::interval(internal.interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = close_rx.recv() =>{
                    return Ok(());
                }
                p = packet_chan_rx.recv() => {
                    if let Some(p) = p {
                        let mut recorder = internal.recorder.lock().await;
                        recorder.record(p.ssrc, p.sequence_number, p.arrival_time);
                    }
                }
                _ = ticker.tick() =>{
                    // build and send twcc
                    let pkts = {
                        let mut recorder = internal.recorder.lock().await;
                        recorder.build_feedback_packet()
                    };

                    if pkts.is_empty() {
                        continue;
                    }

                    if let Err(err) = rtcp_writer.write(&pkts, &a).await{
                        log::error!("rtcp_writer.write got err: {}", err);
                    }
                }
            }
        }
    }
}

#[async_trait]
impl Interceptor for Receiver {
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

        {
            let mut recorder = self.internal.recorder.lock().await;
            *recorder = Recorder::new(rand::random::<u32>());
        }

        let mut w = {
            let wait_group = self.wg.lock().await;
            wait_group.as_ref().map(|wg| wg.worker())
        };
        let writer2 = Arc::clone(&writer);
        let internal = Arc::clone(&self.internal);
        tokio::spawn(async move {
            let _d = w.take();
            if let Err(err) = Receiver::run(writer2, internal).await {
                log::warn!("bind_rtcp_writer TWCC Sender::run got error: {}", err);
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
        let mut hdr_ext_id = 0u8;
        for e in &info.rtp_header_extensions {
            if e.uri == TRANSPORT_CC_URI {
                hdr_ext_id = e.id as u8;
                break;
            }
        }
        if hdr_ext_id == 0 {
            // Don't try to read header extension if ID is 0, because 0 is an invalid extension ID
            return reader;
        }

        let stream = Arc::new(ReceiverStream::new(
            reader,
            hdr_ext_id,
            info.ssrc,
            self.packet_chan_tx.clone(),
            self.start_time,
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
