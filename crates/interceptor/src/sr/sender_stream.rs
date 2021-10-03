use crate::{Attributes, NowFn, RTPWriter};

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::Mutex;

struct SenderStreamInternal {
    clock_rate: f64,

    /// data from rtp packets
    last_rtptime_rtp: u32,
    last_rtptime_time: SystemTime,
    packet_count: u32,
    octet_count: u32,
}

impl SenderStreamInternal {
    fn process_rtp(&mut self, now: SystemTime, pkt: &rtp::packet::Packet) {
        // always update time to minimize errors
        self.last_rtptime_rtp = pkt.header.timestamp;
        self.last_rtptime_time = now;

        self.packet_count += 1;
        self.octet_count += pkt.payload.len() as u32;
    }
}

pub(crate) struct SenderStream {
    next_rtp_writer: Arc<dyn RTPWriter + Send + Sync>,
    now: Option<NowFn>,

    internal: Mutex<SenderStreamInternal>,
}

impl SenderStream {
    pub(crate) fn new(
        clock_rate: u32,
        writer: Arc<dyn RTPWriter + Send + Sync>,
        now: Option<NowFn>,
    ) -> Self {
        SenderStream {
            next_rtp_writer: writer,
            now,

            internal: Mutex::new(SenderStreamInternal {
                clock_rate: clock_rate as f64,
                last_rtptime_rtp: 0,
                last_rtptime_time: SystemTime::UNIX_EPOCH,
                packet_count: 0,
                octet_count: 0,
            }),
        }
    }

    async fn process_rtp(&self, now: SystemTime, pkt: &rtp::packet::Packet) {
        let mut internal = self.internal.lock().await;
        internal.process_rtp(now, pkt);
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
