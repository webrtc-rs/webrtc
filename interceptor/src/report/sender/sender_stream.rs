use super::*;
use crate::{Attributes, RTPWriter};

use async_trait::async_trait;
use rtp::extension::abs_send_time_extension::unix2ntp;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;

struct SenderStreamInternal {
    ssrc: u32,
    clock_rate: f64,

    /// data from rtp packets
    last_rtp_time_rtp: u32,
    last_rtp_time_time: SystemTime,
    packet_count: u32,
    octet_count: u32,
}

impl SenderStreamInternal {
    fn process_rtp(&mut self, now: SystemTime, pkt: &rtp::packet::Packet) {
        // always update time to minimize errors
        self.last_rtp_time_rtp = pkt.header.timestamp;
        self.last_rtp_time_time = now;

        self.packet_count += 1;
        self.octet_count += pkt.payload.len() as u32;
    }

    fn generate_report(&mut self, now: SystemTime) -> rtcp::sender_report::SenderReport {
        rtcp::sender_report::SenderReport {
            ssrc: self.ssrc,
            ntp_time: unix2ntp(now),
            rtp_time: self.last_rtp_time_rtp
                + (now
                    .duration_since(self.last_rtp_time_time)
                    .unwrap_or_else(|_| Duration::from_secs(0))
                    .as_secs_f64()
                    * self.clock_rate) as u32,
            packet_count: self.packet_count,
            octet_count: self.octet_count,
            ..Default::default()
        }
    }
}

pub(crate) struct SenderStream {
    next_rtp_writer: Arc<dyn RTPWriter + Send + Sync>,
    now: Option<FnTimeGen>,

    internal: Mutex<SenderStreamInternal>,
}

impl SenderStream {
    pub(crate) fn new(
        ssrc: u32,
        clock_rate: u32,
        writer: Arc<dyn RTPWriter + Send + Sync>,
        now: Option<FnTimeGen>,
    ) -> Self {
        SenderStream {
            next_rtp_writer: writer,
            now,

            internal: Mutex::new(SenderStreamInternal {
                ssrc,
                clock_rate: clock_rate as f64,
                last_rtp_time_rtp: 0,
                last_rtp_time_time: SystemTime::UNIX_EPOCH,
                packet_count: 0,
                octet_count: 0,
            }),
        }
    }

    async fn process_rtp(&self, now: SystemTime, pkt: &rtp::packet::Packet) {
        let mut internal = self.internal.lock().await;
        internal.process_rtp(now, pkt);
    }

    pub(crate) async fn generate_report(
        &self,
        now: SystemTime,
    ) -> rtcp::sender_report::SenderReport {
        let mut internal = self.internal.lock().await;
        internal.generate_report(now)
    }
}

/// RTPWriter is used by Interceptor.bind_local_stream.
#[async_trait]
impl RTPWriter for SenderStream {
    /// write a rtp packet
    async fn write(&self, pkt: &rtp::packet::Packet, a: &Attributes) -> Result<usize> {
        let now = if let Some(f) = &self.now {
            f().await
        } else {
            SystemTime::now()
        };
        self.process_rtp(now, pkt).await;

        self.next_rtp_writer.write(pkt, a).await
    }
}
