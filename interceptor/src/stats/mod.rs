use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use tokio::time::{Duration, Instant};

mod interceptor;

pub use self::interceptor::StatsInterceptor;

pub fn make_stats_interceptor(id: &str) -> Arc<StatsInterceptor> {
    Arc::new(StatsInterceptor::new(id.to_owned()))
}

#[derive(Debug, Clone)]
pub struct StreamStats {
    rtp_recv_stats: RTPStats,
    rtp_write_stats: RTPStats,
    rtcp_recv_stats: RTCPStats,
    rtcp_write_stats: RTCPStats,

    last_update: Instant,
}

impl Default for StreamStats {
    fn default() -> Self {
        Self {
            rtp_recv_stats: Default::default(),
            rtp_write_stats: Default::default(),
            rtcp_recv_stats: Default::default(),
            rtcp_write_stats: Default::default(),
            last_update: Instant::now(),
        }
    }
}

impl StreamStats {
    pub fn snapshot(&self) -> StatsSnapshot {
        self.into()
    }

    fn mark_updated(&mut self) {
        self.last_update = Instant::now();
    }

    fn duration_since_last_update(&self) -> Duration {
        self.last_update.elapsed()
    }
}

#[derive(Debug)]
pub struct StatsSnapshot {
    pub rtp_recv_stats: RTPStats,
    pub rtp_write_stats: RTPStats,
    pub rtcp_recv_stats: RTCPStats,
    pub rtcp_write_stats: RTCPStats,
}

impl From<&StreamStats> for StatsSnapshot {
    fn from(stats: &StreamStats) -> Self {
        Self {
            rtp_recv_stats: stats.rtp_recv_stats.clone(),
            rtp_write_stats: stats.rtp_write_stats.clone(),
            rtcp_recv_stats: stats.rtcp_recv_stats.clone(),
            rtcp_write_stats: stats.rtcp_write_stats.clone(),
        }
    }
}

#[derive(Default, Debug)]
struct StatsContainer {
    stream_stats: HashMap<u32, StreamStats>,
}

impl StatsContainer {
    fn get_or_create_stream_stats(&mut self, ssrc: u32) -> &mut StreamStats {
        self.stream_stats.entry(ssrc).or_default()
    }

    fn get(&self, ssrc: u32) -> Option<&StreamStats> {
        self.stream_stats.get(&ssrc)
    }

    fn remove_stale_entries(&mut self) {
        self.stream_stats
            .retain(|_, s| s.duration_since_last_update() < Duration::from_secs(60))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
/// Records stats about a given RTP stream.
pub struct RTPStats {
    /// Packets sent or received
    packets: u64,

    /// Payload bytes sent or received
    payload_bytes: u64,

    /// Header bytes sent or received
    header_bytes: u64,

    /// A wall clock timestamp for when the last packet was sent or recieved encoded as milliseconds since
    /// [`SystemTime::UNIX_EPOCH`].
    last_packet_timestamp: Option<SystemTime>,
}

impl RTPStats {
    fn update(&mut self, header_bytes: u64, payload_bytes: u64, packets: u64, now: SystemTime) {
        self.header_bytes += header_bytes;
        self.payload_bytes += payload_bytes;
        self.packets += packets;
        self.last_packet_timestamp = Some(now);
    }

    pub fn header_bytes(&self) -> u64 {
        self.header_bytes
    }

    pub fn payload_bytes(&self) -> u64 {
        self.payload_bytes
    }

    pub fn packets(&self) -> u64 {
        self.packets
    }

    pub fn last_packet_timestamp(&self) -> Option<SystemTime> {
        self.last_packet_timestamp
    }
}

#[derive(Debug, Default, Clone)]
pub struct RTCPStats {
    rtt_ms: f64,
    loss: u8,
    fir_count: u64,
    pli_count: u64,
    nack_count: u64,
}

impl RTCPStats {
    fn update(
        &mut self,
        rtt_ms: Option<f64>,
        loss: Option<u8>,
        fir_count: Option<u64>,
        pli_count: Option<u64>,
        nack_count: Option<u64>,
    ) {
        if let Some(rtt_ms) = rtt_ms {
            self.rtt_ms = rtt_ms;
        }

        if let Some(loss) = loss {
            self.loss = loss;
        }

        if let Some(fir_count) = fir_count {
            self.fir_count += fir_count;
        }

        if let Some(pli_count) = pli_count {
            self.pli_count += pli_count;
        }

        if let Some(nack_count) = nack_count {
            self.nack_count += nack_count;
        }
    }

    pub fn rtt_ms(&self) -> f64 {
        self.rtt_ms
    }

    pub fn loss(&self) -> u8 {
        self.loss
    }

    pub fn fir_count(&self) -> u64 {
        self.fir_count
    }

    pub fn pli_count(&self) -> u64 {
        self.pli_count
    }

    pub fn nack_count(&self) -> u64 {
        self.nack_count
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_rtp_stats() {
        let mut stats: RTPStats = Default::default();
        assert_eq!(
            (stats.header_bytes(), stats.payload_bytes(), stats.packets()),
            (0, 0, 0),
        );

        stats.update(24, 960, 1, SystemTime::now());

        assert_eq!(
            (stats.header_bytes(), stats.payload_bytes(), stats.packets()),
            (24, 960, 1),
        );
    }

    #[test]
    fn test_rtp_stats_send_sync() {
        fn test_send_sync<T: Send + Sync>() {}
        test_send_sync::<RTPStats>();
    }

    #[test]
    fn test_rtcp_stats_send_sync() {
        fn test_send_sync<T: Send + Sync>() {}
        test_send_sync::<RTCPStats>();
    }
}
