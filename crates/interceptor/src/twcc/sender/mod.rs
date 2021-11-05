#[cfg(test)]
mod sender_test;

use crate::twcc::header_extension::TRANSPORT_CC_URI;
use crate::twcc::Recorder;
use crate::*;
use rtp::extension::transport_cc_extension::TransportCcExtension;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, Mutex};
use util::Unmarshal;
use waitgroup::WaitGroup;

/// SenderBuilder is a InterceptorBuilder for a SenderInterceptor
#[derive(Default)]
pub struct SenderBuilder {
    interval: Option<Duration>,
}

impl SenderBuilder {
    /// with_interval sets send interval for the interceptor.
    pub fn with_interval(mut self, interval: Duration) -> SenderBuilder {
        self.interval = Some(interval);
        self
    }
}

impl InterceptorBuilder for SenderBuilder {
    fn build(&self, _id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        let (close_tx, close_rx) = mpsc::channel(1);
        let (packet_chan_tx, packet_chan_rx) = mpsc::channel(1);
        Ok(Arc::new(Sender {
            internal: Arc::new(SenderInternal {
                interval: if let Some(interval) = &self.interval {
                    *interval
                } else {
                    Duration::from_millis(100)
                },
                start_time: SystemTime::now(),

                hdr_ext_id: AtomicU8::new(0),
                ssrc: AtomicU32::new(0),
                parent_rtp_reader: Mutex::new(None),

                recorder: Mutex::new(Recorder::default()),
                packet_chan_tx,
                packet_chan_rx: Mutex::new(Some(packet_chan_rx)),
                close_rx: Mutex::new(Some(close_rx)),
            }),
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

struct SenderInternal {
    interval: Duration,
    start_time: SystemTime,

    hdr_ext_id: AtomicU8,
    ssrc: AtomicU32,
    parent_rtp_reader: Mutex<Option<Arc<dyn RTPReader + Send + Sync>>>,

    recorder: Mutex<Recorder>,
    packet_chan_tx: mpsc::Sender<Packet>,
    packet_chan_rx: Mutex<Option<mpsc::Receiver<Packet>>>,
    close_rx: Mutex<Option<mpsc::Receiver<()>>>,
}

#[async_trait]
impl RTPReader for SenderInternal {
    /// read a rtp packet
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)> {
        let (n, attr) = {
            let parent_rtp_reader = {
                let parent_rtp_reader = self.parent_rtp_reader.lock().await;
                parent_rtp_reader.clone()
            };
            if let Some(reader) = parent_rtp_reader {
                reader.read(buf, attributes).await?
            } else {
                return Err(Error::ErrInvalidParentRtpReader);
            }
        };

        let mut b = &buf[..n];
        let p = rtp::packet::Packet::unmarshal(&mut b)?;

        if let Some(mut ext) = p
            .header
            .get_extension(self.hdr_ext_id.load(Ordering::SeqCst))
        {
            let tcc_ext = TransportCcExtension::unmarshal(&mut ext)?;

            let _ = self
                .packet_chan_tx
                .send(Packet {
                    hdr: p.header,
                    sequence_number: tcc_ext.transport_sequence,
                    arrival_time: SystemTime::now()
                        .duration_since(self.start_time)
                        .unwrap_or_else(|_| Duration::from_secs(0))
                        .as_micros() as i64,
                    ssrc: self.ssrc.load(Ordering::SeqCst),
                })
                .await;
        }

        Ok((n, attr))
    }
}

/// Sender sends transport wide congestion control reports as specified in:
/// https://datatracker.ietf.org/doc/html/draft-holmer-rmcat-transport-wide-cc-extensions-01
pub struct Sender {
    internal: Arc<SenderInternal>,

    wg: Mutex<Option<WaitGroup>>,
    close_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl Sender {
    /// builder returns a new SenderBuilder.
    pub fn builder() -> SenderBuilder {
        SenderBuilder::default()
    }

    async fn is_closed(&self) -> bool {
        let close_tx = self.close_tx.lock().await;
        close_tx.is_none()
    }

    async fn run(
        rtcp_writer: Arc<dyn RTCPWriter + Send + Sync>,
        internal: Arc<SenderInternal>,
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

                    if let Err(err) = rtcp_writer.write(&pkts, &a).await{
                        log::error!("rtcp_writer.write got err: {}", err);
                    }
                }
            }
        }
    }
}

#[async_trait]
impl Interceptor for Sender {
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
            let _ = Sender::run(writer2, internal).await;
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

        {
            self.internal.hdr_ext_id.store(hdr_ext_id, Ordering::SeqCst);
            self.internal.ssrc.store(info.ssrc, Ordering::SeqCst);

            let mut parent_rtp_reader = self.internal.parent_rtp_reader.lock().await;
            *parent_rtp_reader = Some(reader);
        }

        Arc::clone(&self.internal) as Arc<dyn RTPReader + Send + Sync>
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
