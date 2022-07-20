use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::sync::Arc;
use std::time::SystemTime;

use super::{StatsContainer, StatsSnapshot, StreamStats};
use async_trait::async_trait;
use rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use rtcp::receiver_report::ReceiverReport;
use rtcp::transport_feedbacks::transport_layer_nack::TransportLayerNack;
use rtp::extension::abs_send_time_extension::unix2ntp;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;

use util::sync::{Mutex, RwLock};
use util::{MarshalSize, Unmarshal};

use crate::error::Result;
use crate::stream_info::StreamInfo;
use crate::{Attributes, Interceptor, RTCPReader, RTCPWriter, RTPReader, RTPWriter};

#[derive(Debug)]
enum Message {
    StatUpdate {
        ssrc: u32,
        update: StatsUpdate,
    },
    RequestSnapshot {
        ssrcs: Vec<u32>,
        chan: oneshot::Sender<Vec<Option<StatsSnapshot>>>,
    },
}

#[derive(Debug)]
enum StatsUpdate {
    RecvRTP {
        packets: u64,
        header_bytes: u64,
        payload_bytes: u64,
        last_packet_timestamp: SystemTime,
    },
    WriteRTP {
        packets: u64,
        header_bytes: u64,
        payload_bytes: u64,
        last_packet_timestamp: SystemTime,
    },
    RecvRTCP {
        rtt_ms: Option<f64>,
        loss: Option<u8>,
        fir_count: Option<u64>,
        pli_count: Option<u64>,
        nack_count: Option<u64>,
    },
    WriteRTCP {
        rtt_ms: Option<f64>,
        loss: Option<u8>,
        fir_count: Option<u64>,
        pli_count: Option<u64>,
        nack_count: Option<u64>,
    },
}

pub struct StatsInterceptor {
    // Wrapped RTP streams
    recv_streams: Mutex<HashMap<u32, Arc<RTPReadRecorder>>>,
    send_streams: Mutex<HashMap<u32, Arc<RTPWriteRecorder>>>,

    tx: mpsc::Sender<Message>,

    id: String,
    now_gen: Option<Arc<dyn Fn() -> SystemTime + Send + Sync>>,
}

impl StatsInterceptor {
    pub fn new(id: String) -> Self {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(run_stats_reducer(rx));

        Self {
            id,
            recv_streams: Default::default(),
            send_streams: Default::default(),
            tx,
            now_gen: None,
        }
    }

    fn with_time_gen<F>(id: String, now_gen: F) -> Self
    where
        F: Fn() -> SystemTime + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(run_stats_reducer(rx));

        Self {
            id,
            recv_streams: Default::default(),
            send_streams: Default::default(),
            tx,
            now_gen: Some(Arc::new(now_gen)),
        }
    }

    pub async fn fetch_stats(&self, ssrcs: Vec<u32>) -> Vec<Option<StatsSnapshot>> {
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self
            .tx
            .send(Message::RequestSnapshot { ssrcs, chan: tx })
            .await
        {
            log::debug!(
                "Failed to fetch RTP stream stats from stats task with error: {}",
                e
            );

            return vec![];
        }

        rx.await.unwrap_or_default()
    }
}

async fn run_stats_reducer(mut rx: mpsc::Receiver<Message>) {
    let mut ssrc_stats: StatsContainer = Default::default();
    let mut cleanup_ticker = tokio::time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            maybe_msg = rx.recv() => {
                let msg = match maybe_msg {
                    Some(m) => m,
                    None => break,
                };
                match msg {
                    Message::StatUpdate { ssrc, update } => {
                        let stats = ssrc_stats.get_or_create_stream_stats(ssrc);

                        match update {
                            StatsUpdate::RecvRTP {
                                packets,
                                header_bytes,
                                payload_bytes,
                                last_packet_timestamp,
                            }
                            | StatsUpdate::WriteRTP {
                                packets,
                                header_bytes,
                                payload_bytes,
                                last_packet_timestamp,
                            } => {
                                if matches!(update, StatsUpdate::RecvRTP { .. }) {
                                    stats.rtp_recv_stats.update(
                                        header_bytes,
                                        payload_bytes,
                                        packets,
                                        last_packet_timestamp,
                                    );
                                } else {
                                    stats.rtp_write_stats.update(
                                        header_bytes,
                                        payload_bytes,
                                        packets,
                                        last_packet_timestamp,
                                    );
                                }
                            }
                            StatsUpdate::RecvRTCP {
                                rtt_ms,
                                loss,
                                fir_count,
                                pli_count,
                                nack_count,
                            }
                            | StatsUpdate::WriteRTCP {
                                rtt_ms,
                                loss,
                                fir_count,
                                pli_count,
                                nack_count,
                            } => {
                                if matches!(update, StatsUpdate::RecvRTCP { .. }) {
                                    stats
                                        .rtcp_recv_stats
                                        .update(rtt_ms, loss, fir_count, pli_count, nack_count);
                                } else {
                                    stats
                                        .rtcp_write_stats
                                        .update(rtt_ms, loss, fir_count, pli_count, nack_count);
                                }
                            }
                        }

                        stats.mark_updated();
                    }
                    Message::RequestSnapshot { ssrcs, chan } => {
                        let result = ssrcs
                            .into_iter()
                            .map(|ssrc| ssrc_stats.get(ssrc).map(StreamStats::snapshot))
                            .collect();

                        let _ = chan.send(result);
                    }
                }

            }
            _ = cleanup_ticker.tick() => {
                ssrc_stats.remove_stale_entries();
            }
        }
    }
}

#[async_trait]
impl Interceptor for StatsInterceptor {
    /// bind_remote_stream lets you modify any incoming RTP packets. It is called once for per RemoteStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_remote_stream(
        &self,
        info: &StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        let mut lock = self.recv_streams.lock();

        let e = lock
            .entry(info.ssrc)
            .or_insert_with(|| Arc::new(RTPReadRecorder::new(reader, self.tx.clone())));

        e.clone()
    }

    /// unbind_remote_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_remote_stream(&self, info: &StreamInfo) {
        let mut lock = self.recv_streams.lock();

        lock.remove(&info.ssrc);
    }

    /// bind_local_stream lets you modify any outgoing RTP packets. It is called once for per LocalStream. The returned method
    /// will be called once per rtp packet.
    async fn bind_local_stream(
        &self,
        info: &StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        let mut lock = self.send_streams.lock();

        let e = lock
            .entry(info.ssrc)
            .or_insert_with(|| Arc::new(RTPWriteRecorder::new(writer, self.tx.clone())));

        e.clone()
    }

    /// unbind_local_stream is called when the Stream is removed. It can be used to clean up any data related to that track.
    async fn unbind_local_stream(&self, info: &StreamInfo) {
        let mut lock = self.send_streams.lock();

        lock.remove(&info.ssrc);
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }

    /// bind_rtcp_writer lets you modify any outgoing RTCP packets. It is called once per PeerConnection. The returned method
    /// will be called once per packet batch.
    async fn bind_rtcp_writer(
        &self,
        writer: Arc<dyn RTCPWriter + Send + Sync>,
    ) -> Arc<dyn RTCPWriter + Send + Sync> {
        let now = self
            .now_gen
            .clone()
            .unwrap_or_else(|| Arc::new(SystemTime::now));

        Arc::new(RTCPWriteInterceptor {
            rtcp_writer: writer,
            tx: self.tx.clone(),
            now_gen: move || now(),
        })
    }

    /// bind_rtcp_reader lets you modify any incoming RTCP packets. It is called once per sender/receiver, however this might
    /// change in the future. The returned method will be called once per packet batch.
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        let now = self
            .now_gen
            .clone()
            .unwrap_or_else(|| Arc::new(SystemTime::now));

        Arc::new(RTCPReadInterceptor {
            rtcp_reader: reader,
            tx: self.tx.clone(),
            now_gen: move || now(),
        })
    }
}

pub struct RTCPReadInterceptor<F> {
    rtcp_reader: Arc<dyn RTCPReader + Send + Sync>,
    tx: mpsc::Sender<Message>,
    now_gen: F,
}

#[async_trait]
impl<F> RTCPReader for RTCPReadInterceptor<F>
where
    F: Fn() -> SystemTime + Send + Sync,
{
    /// read a batch of rtcp packets
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)> {
        let (n, attributes) = self.rtcp_reader.read(buf, attributes).await?;

        let mut b = &buf[..n];
        let pkts = rtcp::packet::unmarshal(&mut b)?;
        // Middle 32 bits
        let now = (unix2ntp((self.now_gen)()) >> 16) as u32;

        #[derive(Default, Debug)]
        struct Entry {
            rtt_ms: Option<f64>,
            loss: Option<u8>,
            fir_count: Option<u64>,
            pli_count: Option<u64>,
            nack_count: Option<u64>,
        }
        let updates = pkts
            .iter()
            .fold(HashMap::<u32, Entry>::new(), |mut acc, p| {
                if let Some(rr) = p.as_any().downcast_ref::<ReceiverReport>() {
                    for recp in &rr.reports {
                        let rtt_ms = calculate_rtt_ms(now, recp.delay, recp.last_sender_report);

                        let e = acc.entry(recp.ssrc).or_default();
                        e.rtt_ms = Some(rtt_ms);
                        e.loss = Some(recp.fraction_lost);
                    }
                } else if let Some(fir) = p.as_any().downcast_ref::<FullIntraRequest>() {
                    for fir_entry in &fir.fir {
                        let e = acc.entry(fir_entry.ssrc).or_default();
                        e.fir_count = e.fir_count.map(|v| v + 1).or(Some(1));
                    }
                } else if let Some(pli) = p.as_any().downcast_ref::<PictureLossIndication>() {
                    let e = acc.entry(pli.media_ssrc).or_default();
                    e.pli_count = e.pli_count.map(|v| v + 1).or(Some(1));
                } else if let Some(nack) = p.as_any().downcast_ref::<TransportLayerNack>() {
                    let count = nack.nacks.iter().flat_map(|p| p.into_iter()).count() as u64;

                    let e = acc.entry(nack.media_ssrc).or_default();
                    e.nack_count = e.nack_count.map(|v| v + count).or(Some(count));
                }

                acc
            });

        for (
            ssrc,
            Entry {
                rtt_ms,
                loss,
                fir_count,
                pli_count,
                nack_count,
            },
        ) in updates.into_iter()
        {
            let _ = self
                .tx
                .send(Message::StatUpdate {
                    ssrc,
                    update: StatsUpdate::RecvRTCP {
                        rtt_ms,
                        loss,
                        fir_count,
                        pli_count,
                        nack_count,
                    },
                })
                .await;
        }

        Ok((n, attributes))
    }
}

pub struct RTCPWriteInterceptor<F> {
    rtcp_writer: Arc<dyn RTCPWriter + Send + Sync>,
    tx: mpsc::Sender<Message>,
    now_gen: F,
}

#[async_trait]
impl<F> RTCPWriter for RTCPWriteInterceptor<F>
where
    F: Fn() -> SystemTime + Send + Sync,
{
    async fn write(
        &self,
        pkts: &[Box<dyn rtcp::packet::Packet + Send + Sync>],
        attributes: &Attributes,
    ) -> Result<usize> {
        let now = (unix2ntp((self.now_gen)()) >> 16) as u32;

        #[derive(Default, Debug)]
        struct Entry {
            rtt_ms: Option<f64>,
            loss: Option<u8>,
            fir_count: Option<u64>,
            pli_count: Option<u64>,
            nack_count: Option<u64>,
        }
        let updates = pkts
            .iter()
            .fold(HashMap::<u32, Entry>::new(), |mut acc, p| {
                if let Some(rr) = p.as_any().downcast_ref::<ReceiverReport>() {
                    for recp in &rr.reports {
                        let rtt_ms = calculate_rtt_ms(now, recp.delay, recp.last_sender_report);

                        let e = acc.entry(recp.ssrc).or_default();
                        e.rtt_ms = Some(rtt_ms);
                        e.loss = Some(recp.fraction_lost);
                    }
                } else if let Some(fir) = p.as_any().downcast_ref::<FullIntraRequest>() {
                    for fir_entry in &fir.fir {
                        let e = acc.entry(fir_entry.ssrc).or_default();
                        e.fir_count = e.fir_count.map(|v| v + 1).or(Some(1));
                    }
                } else if let Some(pli) = p.as_any().downcast_ref::<PictureLossIndication>() {
                    let e = acc.entry(pli.media_ssrc).or_default();
                    e.pli_count = e.pli_count.map(|v| v + 1).or(Some(1));
                } else if let Some(nack) = p.as_any().downcast_ref::<TransportLayerNack>() {
                    let count = nack.nacks.iter().flat_map(|p| p.into_iter()).count() as u64;

                    let e = acc.entry(nack.media_ssrc).or_default();
                    e.nack_count = e.nack_count.map(|v| v + count).or(Some(count));
                }

                acc
            });

        dbg!(&updates);
        for (
            ssrc,
            Entry {
                rtt_ms,
                loss,
                fir_count,
                pli_count,
                nack_count,
            },
        ) in updates.into_iter()
        {
            let _ = self
                .tx
                .send(Message::StatUpdate {
                    ssrc,
                    update: StatsUpdate::WriteRTCP {
                        rtt_ms,
                        loss,
                        fir_count,
                        pli_count,
                        nack_count,
                    },
                })
                .await;
        }

        self.rtcp_writer.write(pkts, attributes).await
    }
}

pub struct RTPReadRecorder {
    rtp_reader: Arc<dyn RTPReader + Send + Sync>,
    tx: mpsc::Sender<Message>,
}

impl RTPReadRecorder {
    fn new(rtp_reader: Arc<dyn RTPReader + Send + Sync>, tx: mpsc::Sender<Message>) -> Self {
        Self { rtp_reader, tx }
    }
}

impl fmt::Debug for RTPReadRecorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RTPReadRecorder").finish()
    }
}

#[async_trait]
impl RTPReader for RTPReadRecorder {
    async fn read(&self, buf: &mut [u8], attributes: &Attributes) -> Result<(usize, Attributes)> {
        let (bytes_read, attributes) = self.rtp_reader.read(buf, attributes).await?;
        // TODO: This parsing happens redundantly in several interceptors, would be good if we
        // could not do this.
        let mut b = &buf[..bytes_read];
        let packet = rtp::packet::Packet::unmarshal(&mut b)?;

        let _ = self
            .tx
            .send(Message::StatUpdate {
                ssrc: packet.header.ssrc,
                update: StatsUpdate::RecvRTP {
                    packets: 1,
                    header_bytes: (bytes_read - packet.payload.len()) as u64,
                    payload_bytes: packet.payload.len() as u64,
                    last_packet_timestamp: SystemTime::now(),
                },
            })
            .await;

        Ok((bytes_read, attributes))
    }
}

pub struct RTPWriteRecorder {
    rtp_writer: Arc<dyn RTPWriter + Send + Sync>,
    tx: mpsc::Sender<Message>,
}

impl RTPWriteRecorder {
    fn new(rtp_writer: Arc<dyn RTPWriter + Send + Sync>, tx: mpsc::Sender<Message>) -> Self {
        Self { rtp_writer, tx }
    }
}

impl fmt::Debug for RTPWriteRecorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RTPWriteRecorder").finish()
    }
}

#[async_trait]
impl RTPWriter for RTPWriteRecorder {
    /// write a rtp packet
    async fn write(&self, pkt: &rtp::packet::Packet, attributes: &Attributes) -> Result<usize> {
        let n = self.rtp_writer.write(pkt, attributes).await?;

        let _ = self
            .tx
            .send(Message::StatUpdate {
                ssrc: pkt.header.ssrc,
                update: StatsUpdate::WriteRTP {
                    packets: 1,
                    header_bytes: pkt.header.marshal_size() as u64,
                    payload_bytes: pkt.payload.len() as u64,
                    last_packet_timestamp: SystemTime::now(),
                },
            })
            .await;

        Ok(n)
    }
}

trait GetOrCreateAtomic<T, U> {
    fn get_or_create(&self, key: T) -> U;
}

impl<T, U> GetOrCreateAtomic<T, U> for RwLock<HashMap<T, U>>
where
    T: Hash + Eq,
    U: Default + Clone,
{
    fn get_or_create(&self, key: T) -> U {
        let lock = self.read();

        if let Some(v) = lock.get(&key) {
            v.clone()
        } else {
            // Upgrade lock to write
            drop(lock);
            let mut lock = self.write();

            lock.entry(key).or_default().clone()
        }
    }
}

/// Calculate the round trip time for a given peer as described in
/// [RFC3550 6.4.1](https://datatracker.ietf.org/doc/html/rfc3550#section-6.4.1).
///
/// ## Params
///
/// - `now` the current middle 32 bits of an NTP timestamp for the current time.
/// - `delay` the delay(`DLSR`) since last sender report expressed as fractions of a second in 32 bits.
/// - `last_sender_report` the middle 32 bits of an NTP timestamp for the most recent sender report(LSR).
fn calculate_rtt_ms(now: u32, delay: u32, last_sender_report: u32) -> f64 {
    // [10 Nov 1995 11:33:25.125 UTC]       [10 Nov 1995 11:33:36.5 UTC]
    // n                 SR(n)              A=b710:8000 (46864.500 s)
    // ---------------------------------------------------------------->
    //                    v                 ^
    // ntp_sec =0xb44db705 v               ^ dlsr=0x0005:4000 (    5.250s)
    // ntp_frac=0x20000000  v             ^  lsr =0xb705:2000 (46853.125s)
    //   (3024992005.125 s)  v           ^
    // r                      v         ^ RR(n)
    // ---------------------------------------------------------------->
    //                        |<-DLSR->|
    //                         (5.250 s)
    //
    // A     0xb710:8000 (46864.500 s)
    // DLSR -0x0005:4000 (    5.250 s)
    // LSR  -0xb705:2000 (46853.125 s)
    // -------------------------------
    // delay 0x0006:2000 (    6.125 s)

    let rtt = now - delay - last_sender_report;
    let rtt_seconds = rtt >> 16;
    let rtt_fraction = (rtt & (u16::MAX as u32)) as f64 / (u16::MAX as u32) as f64;

    rtt_seconds as f64 * 1000.0 + (rtt_fraction as f64) * 1000.0
}

#[cfg(test)]
mod test {
    use bytes::Bytes;
    use rtcp::payload_feedbacks::full_intra_request::{FirEntry, FullIntraRequest};
    use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
    use rtcp::receiver_report::ReceiverReport;
    use rtcp::reception_report::ReceptionReport;
    use rtcp::transport_feedbacks::transport_layer_nack::{NackPair, TransportLayerNack};

    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use crate::error::Result;
    use crate::mock::mock_stream::MockStream;
    use crate::stream_info::StreamInfo;

    use super::StatsInterceptor;

    #[tokio::test]
    async fn test_stats_interceptor_rtp() -> Result<()> {
        let icpr: Arc<_> = Arc::new(StatsInterceptor::new("Hello".to_owned()));

        let recv_stream = MockStream::new(
            &StreamInfo {
                ssrc: 123456,
                ..Default::default()
            },
            icpr.clone(),
        )
        .await;

        let send_stream = MockStream::new(
            &StreamInfo {
                ssrc: 234567,
                ..Default::default()
            },
            icpr.clone(),
        )
        .await;

        let _ = recv_stream
            .receive_rtp(rtp::packet::Packet {
                header: rtp::header::Header {
                    ssrc: 123456,
                    ..Default::default()
                },
                payload: Bytes::from_static(b"\xde\xad\xbe\xef"),
            })
            .await;

        let _ = recv_stream
            .read_rtp()
            .await
            .expect("After calling receive_rtp read_rtp should return Some")?;

        let _ = send_stream
            .write_rtp(&rtp::packet::Packet {
                header: rtp::header::Header {
                    ssrc: 234567,
                    ..Default::default()
                },
                payload: Bytes::from_static(b"\xde\xad\xbe\xef\xde\xad\xbe\xef"),
            })
            .await;

        let _ = send_stream
            .write_rtp(&rtp::packet::Packet {
                header: rtp::header::Header {
                    ssrc: 234567,
                    ..Default::default()
                },
                payload: Bytes::from_static(&[0x13, 0x37]),
            })
            .await;

        let snapshots = icpr.fetch_stats(vec![123456]).await;
        let recv_snapshot = snapshots[0]
            .as_ref()
            .expect("Stats should exist for ssrc: 123456");
        assert_eq!(recv_snapshot.rtp_recv_stats.packets(), 1);
        assert_eq!(recv_snapshot.rtp_recv_stats.header_bytes(), 12);
        assert_eq!(recv_snapshot.rtp_recv_stats.payload_bytes(), 4);

        let snapshots = icpr.fetch_stats(vec![234567]).await;
        let send_snapshot = snapshots[0]
            .as_ref()
            .expect("Stats should exist for ssrc: 234567");
        assert_eq!(send_snapshot.rtp_write_stats.packets(), 2);
        assert_eq!(send_snapshot.rtp_write_stats.header_bytes(), 24);
        assert_eq!(send_snapshot.rtp_write_stats.payload_bytes(), 10);

        Ok(())
    }

    #[tokio::test]
    async fn test_stats_interceptor_rtcp() -> Result<()> {
        let icpr: Arc<_> = Arc::new(StatsInterceptor::with_time_gen("Hello".to_owned(), || {
            // 10 Nov 1995 11:33:36.5 UTC
            SystemTime::UNIX_EPOCH + Duration::from_secs_f64(816003216.5)
        }));

        let recv_stream = MockStream::new(
            &StreamInfo {
                ssrc: 123456,
                ..Default::default()
            },
            icpr.clone(),
        )
        .await;

        let send_stream = MockStream::new(
            &StreamInfo {
                ssrc: 234567,
                ..Default::default()
            },
            icpr.clone(),
        )
        .await;

        send_stream
            .receive_rtcp(vec![
                Box::new(ReceiverReport {
                    reports: vec![ReceptionReport {
                        ssrc: 234567,
                        last_sender_report: 0xb705_2000,
                        delay: 0x0005_4000,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                Box::new(TransportLayerNack {
                    sender_ssrc: 0,
                    media_ssrc: 234567,
                    nacks: vec![NackPair {
                        packet_id: 5,
                        lost_packets: 0b0011_0110,
                    }],
                }),
                Box::new(TransportLayerNack {
                    sender_ssrc: 0,
                    // NB: Different SSRC
                    media_ssrc: 999999,
                    nacks: vec![NackPair {
                        packet_id: 5,
                        lost_packets: 0b0011_0110,
                    }],
                }),
                Box::new(PictureLossIndication {
                    sender_ssrc: 0,
                    media_ssrc: 234567,
                }),
                Box::new(PictureLossIndication {
                    sender_ssrc: 0,
                    media_ssrc: 234567,
                }),
                Box::new(FullIntraRequest {
                    sender_ssrc: 0,
                    media_ssrc: 234567,
                    fir: vec![
                        FirEntry {
                            ssrc: 234567,
                            sequence_number: 132,
                        },
                        FirEntry {
                            ssrc: 234567,
                            sequence_number: 135,
                        },
                    ],
                }),
            ])
            .await;

        let _ = send_stream
            .read_rtcp()
            .await
            .expect("After calling `receive_rtcp`, `read_rtcp` should return some packets");

        recv_stream
            .write_rtcp(&[
                Box::new(TransportLayerNack {
                    sender_ssrc: 0,
                    media_ssrc: 123456,
                    nacks: vec![NackPair {
                        packet_id: 5,
                        lost_packets: 0b0011_0111,
                    }],
                }),
                Box::new(TransportLayerNack {
                    sender_ssrc: 0,
                    // NB: Different SSRC
                    media_ssrc: 999999,
                    nacks: vec![NackPair {
                        packet_id: 5,
                        lost_packets: 0b1111_0110,
                    }],
                }),
                Box::new(PictureLossIndication {
                    sender_ssrc: 0,
                    media_ssrc: 123456,
                }),
                Box::new(PictureLossIndication {
                    sender_ssrc: 0,
                    media_ssrc: 123456,
                }),
                Box::new(PictureLossIndication {
                    sender_ssrc: 0,
                    media_ssrc: 123456,
                }),
                Box::new(FullIntraRequest {
                    sender_ssrc: 0,
                    media_ssrc: 123456,
                    fir: vec![FirEntry {
                        ssrc: 123456,
                        sequence_number: 132,
                    }],
                }),
            ])
            .await
            .expect("Failed to write RTCP packets for recv_stream");
        let snapshots = icpr.fetch_stats(vec![234567]).await;

        let send_snapshot = snapshots[0]
            .as_ref()
            .expect("Stats should exist for ssrc: 234567");
        let rtt_ms = send_snapshot.rtcp_recv_stats.rtt_ms;
        assert!(
            rtt_ms > 6124.99 && rtt_ms < 6125.01,
            "Expected rtt_ms to be about ~6125.0 it was {}",
            rtt_ms
        );

        assert_eq!(send_snapshot.rtcp_recv_stats.nack_count(), 5);
        assert_eq!(send_snapshot.rtcp_recv_stats.pli_count(), 2);
        assert_eq!(send_snapshot.rtcp_recv_stats.fir_count(), 2);

        let snapshots = icpr.fetch_stats(vec![123456]).await;
        let recv_snapshot = snapshots[0]
            .as_ref()
            .expect("Stats should exist for ssrc: 123456");
        assert_eq!(recv_snapshot.rtcp_write_stats.fir_count(), 1);
        assert_eq!(recv_snapshot.rtcp_write_stats.nack_count(), 6);
        assert_eq!(recv_snapshot.rtcp_write_stats.pli_count(), 3);

        Ok(())
    }
}
