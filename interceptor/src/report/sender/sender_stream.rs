use std::convert::TryInto;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use rtp::extension::abs_send_time_extension::unix2ntp;
use tokio::sync::Mutex;

use super::*;
use crate::{Attributes, RTPWriter};

struct SenderStreamInternal {
    ssrc: u32,
    clock_rate: f64,

    /// data from rtp packets
    last_rtp_time_rtp: u32,
    last_rtp_time_time: SystemTime,
    counters: Counters,
}

impl SenderStreamInternal {
    fn process_rtp(&mut self, now: SystemTime, pkt: &rtp::packet::Packet) {
        // always update time to minimize errors
        self.last_rtp_time_rtp = pkt.header.timestamp;
        self.last_rtp_time_time = now;

        self.counters.increment_packets();
        self.counters.count_octets(pkt.payload.len());
    }

    fn generate_report(&mut self, now: SystemTime) -> rtcp::sender_report::SenderReport {
        rtcp::sender_report::SenderReport {
            ssrc: self.ssrc,
            ntp_time: unix2ntp(now),
            rtp_time: self.last_rtp_time_rtp.wrapping_add(
                (now.duration_since(self.last_rtp_time_time)
                    .unwrap_or_else(|_| Duration::from_secs(0))
                    .as_secs_f64()
                    * self.clock_rate) as u32,
            ),
            packet_count: self.counters.packet_count(),
            octet_count: self.counters.octet_count(),
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
                counters: Default::default(),
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
            f()
        } else {
            SystemTime::now()
        };
        self.process_rtp(now, pkt).await;

        self.next_rtp_writer.write(pkt, a).await
    }
}

#[derive(Default)]
pub(crate) struct Counters {
    packets: u32,
    octets: u32,
}

/// Wrapping counters used for generating [`rtcp::sender_report::SenderReport`]
impl Counters {
    pub fn increment_packets(&mut self) {
        self.packets = self.packets.wrapping_add(1);
    }

    pub fn count_octets(&mut self, octets: usize) {
        // account for a payload size of at most `u32::MAX`
        // and log a message if larger
        self.octets = self
            .octets
            .wrapping_add(octets.try_into().unwrap_or_else(|_| {
                log::warn!("packet payload larger than 32 bits");
                u32::MAX
            }));
    }

    pub fn packet_count(&self) -> u32 {
        self.packets
    }

    pub fn octet_count(&self) -> u32 {
        self.octets
    }

    #[cfg(test)]
    pub fn mock(packets: u32, octets: u32) -> Self {
        Self { packets, octets }
    }
}
