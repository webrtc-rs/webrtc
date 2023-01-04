use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use super::{inbound, outbound, StatsContainer};
use async_trait::async_trait;
use rtcp::extended_report::{DLRRReportBlock, ExtendedReport};
use rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use rtcp::receiver_report::ReceiverReport;
use rtcp::sender_report::SenderReport;
use rtcp::transport_feedbacks::transport_layer_nack::TransportLayerNack;
use rtp::extension::abs_send_time_extension::unix2ntp;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;

use util::sync::Mutex;
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
    RequestInboundSnapshot {
        ssrcs: Vec<u32>,
        chan: oneshot::Sender<Vec<Option<inbound::StatsSnapshot>>>,
    },
    RequestOutboundSnapshot {
        ssrcs: Vec<u32>,
        chan: oneshot::Sender<Vec<Option<outbound::StatsSnapshot>>>,
    },
}

#[derive(Debug)]
enum StatsUpdate {
    /// Stats collected on the receiving end(inbound) of an RTP stream.
    InboundRTP {
        packets: u64,
        header_bytes: u64,
        payload_bytes: u64,
        last_packet_timestamp: SystemTime,
    },
    /// Stats collected on the sending end(outbound) of an RTP stream.
    OutboundRTP {
        packets: u64,
        header_bytes: u64,
        payload_bytes: u64,
        last_packet_timestamp: SystemTime,
    },
    /// Stats collected from received RTCP packets.
    InboundRTCP {
        fir_count: Option<u64>,
        pli_count: Option<u64>,
        nack_count: Option<u64>,
    },
    /// Stats collected from sent RTCP packets.
    OutboundRTCP {
        fir_count: Option<u64>,
        pli_count: Option<u64>,
        nack_count: Option<u64>,
    },
    /// An extended sequence number sent in an SR.
    OutboundSRExtSeqNum { seq_num: u32 },
    /// Stats collected from received Receiver Reports i.e. where we have an outbound RTP stream.
    InboundRecieverReport {
        ext_seq_num: u32,
        total_lost: u32,
        jitter: u32,
        rtt_ms: Option<f64>,
        fraction_lost: u8,
    },
    /// Stats collected from recieved Sender Reports i.e. where we have an inbound RTP stream.
    InboundSenderRerport {
        packets_and_bytes_sent: Option<(u32, u32)>,
        rtt_ms: Option<f64>,
    },
}

pub struct StatsInterceptor {
    // Wrapped RTP streams
    recv_streams: Mutex<HashMap<u32, Arc<RTPReadRecorder>>>,
    send_streams: Mutex<HashMap<u32, Arc<RTPWriteRecorder>>>,

    tx: mpsc::Sender<Message>,

    id: String,
    now_gen: Arc<dyn Fn() -> SystemTime + Send + Sync>,
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
            now_gen: Arc::new(SystemTime::now),
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
            now_gen: Arc::new(now_gen),
        }
    }

    pub async fn fetch_inbound_stats(
        &self,
        ssrcs: Vec<u32>,
    ) -> Vec<Option<inbound::StatsSnapshot>> {
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self
            .tx
            .send(Message::RequestInboundSnapshot { ssrcs, chan: tx })
            .await
        {
            log::debug!(
                "Failed to fetch inbound RTP stream stats from stats task with error: {}",
                e
            );

            return vec![];
        }

        rx.await.unwrap_or_default()
    }

    pub async fn fetch_outbound_stats(
        &self,
        ssrcs: Vec<u32>,
    ) -> Vec<Option<outbound::StatsSnapshot>> {
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self
            .tx
            .send(Message::RequestOutboundSnapshot { ssrcs, chan: tx })
            .await
        {
            log::debug!(
                "Failed to fetch outbound RTP stream stats from stats task with error: {}",
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
                        handle_stats_update(&mut ssrc_stats, ssrc, update);
                    }
                    Message::RequestInboundSnapshot { ssrcs, chan} => {
                        let result = ssrcs
                            .into_iter()
                            .map(|ssrc| ssrc_stats.get_inbound_stats(ssrc).map(inbound::StreamStats::snapshot))
                            .collect();

                        let _ = chan.send(result);
                    }
                    Message::RequestOutboundSnapshot { ssrcs, chan} => {
                        let result = ssrcs
                            .into_iter()
                            .map(|ssrc| ssrc_stats.get_outbound_stats(ssrc).map(outbound::StreamStats::snapshot))
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

fn handle_stats_update(ssrc_stats: &mut StatsContainer, ssrc: u32, update: StatsUpdate) {
    match update {
        StatsUpdate::InboundRTP {
            packets,
            header_bytes,
            payload_bytes,
            last_packet_timestamp,
        } => {
            let stats = ssrc_stats.get_or_create_inbound_stream_stats(ssrc);

            stats
                .rtp_stats
                .update(header_bytes, payload_bytes, packets, last_packet_timestamp);
            stats.mark_updated();
        }
        StatsUpdate::OutboundRTP {
            packets,
            header_bytes,
            payload_bytes,
            last_packet_timestamp,
        } => {
            let stats = ssrc_stats.get_or_create_outbound_stream_stats(ssrc);
            stats
                .rtp_stats
                .update(header_bytes, payload_bytes, packets, last_packet_timestamp);
            stats.mark_updated();
        }
        StatsUpdate::InboundRTCP {
            fir_count,
            pli_count,
            nack_count,
        } => {
            let stats = ssrc_stats.get_or_create_outbound_stream_stats(ssrc);
            stats.rtcp_stats.update(fir_count, pli_count, nack_count);
            stats.mark_updated();
        }
        StatsUpdate::OutboundRTCP {
            fir_count,
            pli_count,
            nack_count,
        } => {
            let stats = ssrc_stats.get_or_create_inbound_stream_stats(ssrc);
            stats.rtcp_stats.update(fir_count, pli_count, nack_count);
            stats.mark_updated();
        }
        StatsUpdate::OutboundSRExtSeqNum { seq_num } => {
            let stats = ssrc_stats.get_or_create_outbound_stream_stats(ssrc);
            stats.record_sr_ext_seq_num(seq_num);
            stats.mark_updated();
        }
        StatsUpdate::InboundRecieverReport {
            ext_seq_num,
            total_lost,
            jitter,
            rtt_ms,
            fraction_lost,
        } => {
            let stats = ssrc_stats.get_or_create_outbound_stream_stats(ssrc);
            stats.record_remote_round_trip_time(rtt_ms);
            stats.update_remote_fraction_lost(fraction_lost);
            stats.update_remote_total_lost(total_lost);
            stats.update_remote_inbound_packets_received(ext_seq_num, total_lost);
            stats.update_remote_jitter(jitter);

            stats.mark_updated();
        }
        StatsUpdate::InboundSenderRerport {
            rtt_ms,
            packets_and_bytes_sent,
        } => {
            // This is a sender report we received, as such it concerns an RTP stream that's
            // outbound at the remote.
            let stats = ssrc_stats.get_or_create_inbound_stream_stats(ssrc);

            if let Some((packets_sent, bytes_sent)) = packets_and_bytes_sent {
                stats.record_sender_report(packets_sent, bytes_sent);
            }
            stats.record_remote_round_trip_time(rtt_ms);

            stats.mark_updated();
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
        let now = self.now_gen.clone();

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
        let now = self.now_gen.clone();

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
        struct GenericRTCP {
            fir_count: Option<u64>,
            pli_count: Option<u64>,
            nack_count: Option<u64>,
        }

        #[derive(Default, Debug)]
        struct ReceiverReportEntry {
            /// Extended sequence number value from Receiver Report, used to calculate remote
            /// stats.
            ext_seq_num: u32,
            /// Total loss value from Receiver Report, used to calculate remote
            /// stats.
            total_lost: u32,
            /// Jitter from Receiver Report.
            jitter: u32,
            /// Round Trip Time calculated from Receiver Report.
            rtt_ms: Option<f64>,
            /// Fraction of packets lost.
            fraction_lost: u8,
        }

        #[derive(Default, Debug)]
        struct SenderReportEntry {
            /// NTP timestamp(from Sender Report).
            sr_ntp_time: Option<u64>,
            /// Packets Sent(from Sender Report).
            sr_packets_sent: Option<u32>,
            /// Bytes Sent(from Sender Report).
            sr_bytes_sent: Option<u32>,
            /// Last RR timestamp(middle bits) from DLRR extended report block.
            dlrr_last_rr: Option<u32>,
            /// Delay since last RR from DLRR extended report block.
            dlrr_delay_rr: Option<u32>,
        }

        #[derive(Default, Debug)]
        struct Entry {
            generic_rtcp: GenericRTCP,
            receiver_reports: Vec<ReceiverReportEntry>,
            sender_reports: Vec<SenderReportEntry>,
        }
        let updates = pkts
            .iter()
            .fold(HashMap::<u32, Entry>::new(), |mut acc, p| {
                if let Some(rr) = p.as_any().downcast_ref::<ReceiverReport>() {
                    for recp in &rr.reports {
                        let e = acc.entry(recp.ssrc).or_default();

                        let rtt_ms = if recp.delay != 0 {
                            calculate_rtt_ms(now, recp.delay, recp.last_sender_report)
                        } else {
                            None
                        };

                        e.receiver_reports.push(ReceiverReportEntry {
                            ext_seq_num: recp.last_sequence_number,
                            total_lost: recp.total_lost,
                            jitter: recp.jitter,
                            rtt_ms,
                            fraction_lost: recp.fraction_lost,
                        });
                    }
                } else if let Some(fir) = p.as_any().downcast_ref::<FullIntraRequest>() {
                    for fir_entry in &fir.fir {
                        let e = acc.entry(fir_entry.ssrc).or_default();
                        e.generic_rtcp.fir_count =
                            e.generic_rtcp.fir_count.map(|v| v + 1).or(Some(1));
                    }
                } else if let Some(pli) = p.as_any().downcast_ref::<PictureLossIndication>() {
                    let e = acc.entry(pli.media_ssrc).or_default();
                    e.generic_rtcp.pli_count = e.generic_rtcp.pli_count.map(|v| v + 1).or(Some(1));
                } else if let Some(nack) = p.as_any().downcast_ref::<TransportLayerNack>() {
                    let count = nack.nacks.iter().flat_map(|p| p.into_iter()).count() as u64;

                    let e = acc.entry(nack.media_ssrc).or_default();
                    e.generic_rtcp.nack_count =
                        e.generic_rtcp.nack_count.map(|v| v + count).or(Some(count));
                } else if let Some(sr) = p.as_any().downcast_ref::<SenderReport>() {
                    let e = acc.entry(sr.ssrc).or_default();
                    let sr_e = {
                        let need_new_entry = e
                            .sender_reports
                            .last()
                            .map(|e| e.sr_packets_sent.is_some())
                            .unwrap_or(true);

                        if need_new_entry {
                            e.sender_reports.push(Default::default());
                        }

                        // SAFETY: Unrwap ok because we just added an entry above
                        e.sender_reports.last_mut().unwrap()
                    };

                    sr_e.sr_ntp_time = Some(sr.ntp_time);
                    sr_e.sr_packets_sent = Some(sr.packet_count);
                    sr_e.sr_bytes_sent = Some(sr.octet_count);
                } else if let Some(xr) = p.as_any().downcast_ref::<ExtendedReport>() {
                    // Extended Report(XR)

                    // We only care about DLRR reports
                    let dlrrs = xr.reports.iter().flat_map(|report| {
                        let dlrr = report.as_any().downcast_ref::<DLRRReportBlock>();

                        dlrr.map(|b| b.reports.iter()).into_iter().flatten()
                    });

                    for dlrr in dlrrs {
                        let e = acc.entry(dlrr.ssrc).or_default();
                        let sr_e = {
                            let need_new_entry = e
                                .sender_reports
                                .last()
                                .map(|e| e.dlrr_last_rr.is_some())
                                .unwrap_or(true);

                            if need_new_entry {
                                e.sender_reports.push(Default::default());
                            }

                            // SAFETY: Unrwap ok because we just added an entry above
                            e.sender_reports.last_mut().unwrap()
                        };

                        sr_e.dlrr_last_rr = Some(dlrr.last_rr);
                        sr_e.dlrr_delay_rr = Some(dlrr.dlrr);
                    }
                }

                acc
            });

        for (
            ssrc,
            Entry {
                generic_rtcp,
                mut receiver_reports,
                mut sender_reports,
            },
        ) in updates.into_iter()
        {
            // Sort RR by seq number low to high
            receiver_reports.sort_by(|a, b| a.ext_seq_num.cmp(&b.ext_seq_num));
            // Sort SR by ntp time, low to high
            sender_reports
                .sort_by(|a, b| a.sr_ntp_time.unwrap_or(0).cmp(&b.sr_ntp_time.unwrap_or(0)));

            let _ = self
                .tx
                .send(Message::StatUpdate {
                    ssrc,
                    update: StatsUpdate::InboundRTCP {
                        fir_count: generic_rtcp.fir_count,
                        pli_count: generic_rtcp.pli_count,
                        nack_count: generic_rtcp.nack_count,
                    },
                })
                .await;

            let futures = receiver_reports.into_iter().map(|rr| {
                self.tx.send(Message::StatUpdate {
                    ssrc,
                    update: StatsUpdate::InboundRecieverReport {
                        ext_seq_num: rr.ext_seq_num,
                        total_lost: rr.total_lost,
                        jitter: rr.jitter,
                        rtt_ms: rr.rtt_ms,
                        fraction_lost: rr.fraction_lost,
                    },
                })
            });
            for fut in futures {
                // TODO: Use futures::join_all
                let _ = fut.await;
            }

            let futures = sender_reports.into_iter().map(|sr| {
                let rtt_ms = match (sr.dlrr_last_rr, sr.dlrr_delay_rr, sr.sr_packets_sent) {
                    (Some(last_rr), Some(delay_rr), Some(_)) if last_rr != 0 && delay_rr != 0 => {
                        calculate_rtt_ms(now, delay_rr, last_rr)
                    }
                    _ => None,
                };

                self.tx.send(Message::StatUpdate {
                    ssrc,
                    update: StatsUpdate::InboundSenderRerport {
                        packets_and_bytes_sent: sr
                            .sr_packets_sent
                            .and_then(|ps| sr.sr_bytes_sent.map(|bs| (ps, bs))),
                        rtt_ms,
                    },
                })
            });
            for fut in futures {
                // TODO: Use futures::join_all
                let _ = fut.await;
            }
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
        #[derive(Default, Debug)]
        struct Entry {
            fir_count: Option<u64>,
            pli_count: Option<u64>,
            nack_count: Option<u64>,
            sr_ext_seq_num: Option<u32>,
        }
        let updates = pkts
            .iter()
            .fold(HashMap::<u32, Entry>::new(), |mut acc, p| {
                if let Some(fir) = p.as_any().downcast_ref::<FullIntraRequest>() {
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
                } else if let Some(sr) = p.as_any().downcast_ref::<SenderReport>() {
                    for rep in &sr.reports {
                        let e = acc.entry(rep.ssrc).or_default();

                        match e.sr_ext_seq_num {
                            // We want the initial value for `last_sequence_number` from the first
                            // SR. It's possible that an RTCP batch contains more than one SR, in
                            // which case we should use the lowest value.
                            Some(seq_num) if seq_num > rep.last_sequence_number => {
                                e.sr_ext_seq_num = Some(rep.last_sequence_number)
                            }
                            None => e.sr_ext_seq_num = Some(rep.last_sequence_number),
                            _ => {}
                        }
                    }
                }

                acc
            });

        for (
            ssrc,
            Entry {
                fir_count,
                pli_count,
                nack_count,
                sr_ext_seq_num,
            },
        ) in updates.into_iter()
        {
            let _ = self
                .tx
                .send(Message::StatUpdate {
                    ssrc,
                    update: StatsUpdate::OutboundRTCP {
                        fir_count,
                        pli_count,
                        nack_count,
                    },
                })
                .await;

            if let Some(seq_num) = sr_ext_seq_num {
                let _ = self
                    .tx
                    .send(Message::StatUpdate {
                        ssrc,
                        update: StatsUpdate::OutboundSRExtSeqNum { seq_num },
                    })
                    .await;
            }
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
                update: StatsUpdate::InboundRTP {
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
                update: StatsUpdate::OutboundRTP {
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

/// Calculate the round trip time for a given peer as described in
/// [RFC3550 6.4.1](https://datatracker.ietf.org/doc/html/rfc3550#section-6.4.1).
///
/// ## Params
///
/// - `now` the current middle 32 bits of an NTP timestamp for the current time.
/// - `delay` the delay(`DLSR`) since last sender report expressed as fractions of a second in 32 bits.
/// - `last_report` the middle 32 bits of an NTP timestamp for the most recent sender report(LSR) or Receiver Report(LRR).
fn calculate_rtt_ms(now: u32, delay: u32, last_report: u32) -> Option<f64> {
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

    let rtt = now.checked_sub(delay)?.checked_sub(last_report)?;
    let rtt_seconds = rtt >> 16;
    let rtt_fraction = (rtt & (u16::MAX as u32)) as f64 / (u16::MAX as u32) as f64;

    Some(rtt_seconds as f64 * 1000.0 + rtt_fraction * 1000.0)
}

#[cfg(test)]
mod test {
    // Silence warning on `..Default::default()` with no effect:
    #![allow(clippy::needless_update)]

    macro_rules! assert_feq {
        ($left: expr, $right: expr) => {
            assert_feq!($left, $right, 0.01);
        };
        ($left: expr, $right: expr, $eps: expr) => {
            if ($left - $right).abs() >= $eps {
                panic!("{:?} was not within {:?} of {:?}", $left, $eps, $right);
            }
        };
    }

    use bytes::Bytes;
    use rtcp::extended_report::{DLRRReport, DLRRReportBlock, ExtendedReport};
    use rtcp::payload_feedbacks::full_intra_request::{FirEntry, FullIntraRequest};
    use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
    use rtcp::receiver_report::ReceiverReport;
    use rtcp::reception_report::ReceptionReport;
    use rtcp::sender_report::SenderReport;
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

        recv_stream
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

        let snapshots = icpr.fetch_inbound_stats(vec![123456]).await;
        let recv_snapshot = snapshots[0]
            .as_ref()
            .expect("Stats should exist for ssrc: 123456");
        assert_eq!(recv_snapshot.packets_received(), 1);
        assert_eq!(recv_snapshot.header_bytes_received(), 12);
        assert_eq!(recv_snapshot.payload_bytes_received(), 4);

        let snapshots = icpr.fetch_outbound_stats(vec![234567]).await;
        let send_snapshot = snapshots[0]
            .as_ref()
            .expect("Stats should exist for ssrc: 234567");
        assert_eq!(send_snapshot.packets_sent(), 2);
        assert_eq!(send_snapshot.header_bytes_sent(), 24);
        assert_eq!(send_snapshot.payload_bytes_sent(), 10);

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
            .write_rtcp(&[Box::new(SenderReport {
                ssrc: 234567,
                reports: vec![
                    ReceptionReport {
                        ssrc: 234567,
                        last_sequence_number: (5 << 16) | 10,
                        ..Default::default()
                    },
                    ReceptionReport {
                        ssrc: 234567,
                        last_sequence_number: (5 << 16) | 85,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            })])
            .await
            .expect("Failed to write RTCP packets");

        send_stream
            .receive_rtcp(vec![
                Box::new(ReceiverReport {
                    reports: vec![
                        ReceptionReport {
                            ssrc: 234567,
                            last_sequence_number: (5 << 16) | 64,
                            total_lost: 5,
                            ..Default::default()
                        },
                        ReceptionReport {
                            ssrc: 234567,
                            last_sender_report: 0xb705_2000,
                            delay: 0x0005_4000,
                            last_sequence_number: (5 << 16) | 70,
                            total_lost: 8,
                            fraction_lost: 32,
                            jitter: 2250,
                            ..Default::default()
                        },
                    ],
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
        let snapshots = icpr.fetch_outbound_stats(vec![234567]).await;
        let send_snapshot = snapshots[0]
            .as_ref()
            .expect("Outbound Stats should exist for ssrc: 234567");

        assert!(
            send_snapshot.remote_round_trip_time().is_none()
                && send_snapshot.remote_round_trip_time_measurements() == 0,
            "Before receiving the first RR we should not have a remote round trip time"
        );
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

        recv_stream
            .receive_rtcp(vec![
                Box::new(SenderReport {
                    ssrc: 123456,
                    ntp_time: 12345, // Used for ordering
                    packet_count: 52,
                    octet_count: 8172,
                    reports: vec![],
                    ..Default::default()
                }),
                Box::new(SenderReport {
                    ssrc: 123456,
                    ntp_time: 23456, // Used for ordering
                    packet_count: 82,
                    octet_count: 10351,
                    reports: vec![],
                    ..Default::default()
                }),
                Box::new(ExtendedReport {
                    sender_ssrc: 928191,
                    reports: vec![Box::new(DLRRReportBlock {
                        reports: vec![DLRRReport {
                            ssrc: 123456,
                            last_rr: 0xb705_2000,
                            dlrr: 0x0005_4000,
                        }],
                    })],
                }),
                Box::new(SenderReport {
                    /// NB: Different SSRC
                    ssrc: 9999999,
                    ntp_time: 99999, // Used for ordering
                    packet_count: 1231,
                    octet_count: 193812,
                    reports: vec![],
                    ..Default::default()
                }),
            ])
            .await;

        let snapshots = icpr.fetch_inbound_stats(vec![123456]).await;
        let recv_snapshot = snapshots[0]
            .as_ref()
            .expect("Stats should exist for ssrc: 123456");
        assert!(
            recv_snapshot.remote_round_trip_time().is_none()
                && recv_snapshot.remote_round_trip_time_measurements() == 0,
            "Before receiving the first SR/DLRR we should not have a remote round trip time"
        );

        let _ = recv_stream.read_rtcp().await.expect("read_rtcp failed");

        let snapshots = icpr.fetch_outbound_stats(vec![234567]).await;
        let send_snapshot = snapshots[0]
            .as_ref()
            .expect("Outbound Stats should exist for ssrc: 234567");
        let rtt_ms = send_snapshot.remote_round_trip_time().expect(
            "After receiving an RR with a DSLR block we should have a remote round trip time",
        );
        assert_feq!(rtt_ms, 6125.0);

        assert_eq!(send_snapshot.nacks_received(), 5);
        assert_eq!(send_snapshot.plis_received(), 2);
        assert_eq!(send_snapshot.firs_received(), 2);
        // Last Seq Num(RR)  - total lost(RR) - Initial Seq Num(SR) + 1
        // 70 - 8 - 10 + 1 = 53
        assert_eq!(send_snapshot.remote_packets_received(), 53);
        assert_feq!(
            send_snapshot
                .remote_fraction_lost()
                .expect("Should have a fraction lost values after receiving RR"),
            32.0 / 256.0
        );
        assert_eq!(send_snapshot.remote_total_lost(), 8);
        assert_eq!(send_snapshot.remote_jitter(), 2250);

        let snapshots = icpr.fetch_inbound_stats(vec![123456]).await;
        let recv_snapshot = snapshots[0]
            .as_ref()
            .expect("Stats should exist for ssrc: 123456");
        assert_eq!(recv_snapshot.nacks_sent(), 6);
        assert_eq!(recv_snapshot.plis_sent(), 3);
        assert_eq!(recv_snapshot.firs_sent(), 1);
        assert_eq!(recv_snapshot.remote_packets_sent(), 82);
        assert_eq!(recv_snapshot.remote_bytes_sent(), 10351);
        let rtt_ms = recv_snapshot
            .remote_round_trip_time()
            .expect("After reciving SR and DLRR we should have a round trip time ");
        assert_feq!(rtt_ms, 6125.0);
        assert_eq!(recv_snapshot.remote_reports_sent(), 2);
        assert_eq!(recv_snapshot.remote_round_trip_time_measurements(), 1);
        assert_feq!(recv_snapshot.remote_total_round_trip_time(), 6125.0);

        Ok(())
    }
}
