use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use tokio::time::Duration;

mod interceptor;

pub use self::interceptor::StatsInterceptor;

pub fn make_stats_interceptor(id: &str) -> Arc<StatsInterceptor> {
    Arc::new(StatsInterceptor::new(id.to_owned()))
}

/// Types related to inbound RTP streams.
mod inbound {
    use std::time::SystemTime;

    use tokio::time::{Duration, Instant};

    use super::{RTCPStats, RTPStats};

    #[derive(Debug, Clone)]
    /// Stats collected for an inbound RTP stream.
    /// Contains both stats relating to the inbound stream and remote stats for the corresponding
    /// outbound stream at the remote end.
    pub(super) struct StreamStats {
        /// Received RTP stats.
        pub(super) rtp_stats: RTPStats,
        /// Common RTCP stats derived from inbound and outbound RTCP packets.
        pub(super) rtcp_stats: RTCPStats,

        /// The last time any stats where update, used for garbage collection to remove obsolete stats.
        last_update: Instant,

        /// The number of packets sent as reported in the latest SR from the remote.
        remote_packets_sent: u32,

        /// The number of bytes sent as reported in the latest SR from the remote.
        remote_bytes_sent: u32,

        /// The total number of sender reports sent by the remote and received.
        remote_reports_sent: u64,

        /// The last remote round trip time measurement in ms. [`None`] if no round trip time has
        /// been derived yet, or if it wasn't possible to derive it.
        remote_round_trip_time: Option<f64>,

        /// The cumulative total round trip times reported in ms.
        remote_total_round_trip_time: f64,

        /// The total number of measurements of the remote round trip time.
        remote_round_trip_time_measurements: u64,
    }

    impl Default for StreamStats {
        fn default() -> Self {
            Self {
                rtp_stats: RTPStats::default(),
                rtcp_stats: RTCPStats::default(),
                last_update: Instant::now(),
                remote_packets_sent: 0,
                remote_bytes_sent: 0,
                remote_reports_sent: 0,
                remote_round_trip_time: None,
                remote_total_round_trip_time: 0.0,
                remote_round_trip_time_measurements: 0,
            }
        }
    }

    impl StreamStats {
        pub(super) fn snapshot(&self) -> StatsSnapshot {
            self.into()
        }

        pub(super) fn mark_updated(&mut self) {
            self.last_update = Instant::now();
        }

        pub(super) fn duration_since_last_update(&self) -> Duration {
            self.last_update.elapsed()
        }

        pub(super) fn record_sender_report(&mut self, packets_sent: u32, bytes_sent: u32) {
            self.remote_reports_sent += 1;
            self.remote_packets_sent = packets_sent;
            self.remote_bytes_sent = bytes_sent;
        }

        pub(super) fn record_remote_round_trip_time(&mut self, round_trip_time: Option<f64>) {
            // Store the latest measurement, even if it's None.
            self.remote_round_trip_time = round_trip_time;

            if let Some(rtt) = round_trip_time {
                // Only if we have a valid measurement do we update the totals
                self.remote_total_round_trip_time += rtt;
                self.remote_round_trip_time_measurements += 1;
            }
        }
    }

    /// A point in time snapshot of the stream stats for an inbound RTP stream.
    ///
    /// Created by [`StreamStats::snapshot`].
    #[derive(Debug)]
    pub struct StatsSnapshot {
        /// Received RTP stats.
        rtp_stats: RTPStats,
        /// Common RTCP stats derived from inbound and outbound RTCP packets.
        rtcp_stats: RTCPStats,

        /// The number of packets sent as reported in the latest SR from the remote.
        remote_packets_sent: u32,

        /// The number of bytes sent as reported in the latest SR from the remote.
        remote_bytes_sent: u32,

        /// The total number of sender reports sent by the remote and received.
        remote_reports_sent: u64,

        /// The last remote round trip time measurement in ms. [`None`] if no round trip time has
        /// been derived yet, or if it wasn't possible to derive it.
        remote_round_trip_time: Option<f64>,

        /// The cumulative total round trip times reported in ms.
        remote_total_round_trip_time: f64,

        /// The total number of measurements of the remote round trip time.
        remote_round_trip_time_measurements: u64,
    }

    impl StatsSnapshot {
        pub fn packets_received(&self) -> u64 {
            self.rtp_stats.packets
        }

        pub fn payload_bytes_received(&self) -> u64 {
            self.rtp_stats.payload_bytes
        }

        pub fn header_bytes_received(&self) -> u64 {
            self.rtp_stats.header_bytes
        }

        pub fn last_packet_received_timestamp(&self) -> Option<SystemTime> {
            self.rtp_stats.last_packet_timestamp
        }

        pub fn nacks_sent(&self) -> u64 {
            self.rtcp_stats.nack_count
        }

        pub fn firs_sent(&self) -> u64 {
            self.rtcp_stats.fir_count
        }

        pub fn plis_sent(&self) -> u64 {
            self.rtcp_stats.pli_count
        }
        pub fn remote_packets_sent(&self) -> u32 {
            self.remote_packets_sent
        }

        pub fn remote_bytes_sent(&self) -> u32 {
            self.remote_bytes_sent
        }

        pub fn remote_reports_sent(&self) -> u64 {
            self.remote_reports_sent
        }

        pub fn remote_round_trip_time(&self) -> Option<f64> {
            self.remote_round_trip_time
        }

        pub fn remote_total_round_trip_time(&self) -> f64 {
            self.remote_total_round_trip_time
        }

        pub fn remote_round_trip_time_measurements(&self) -> u64 {
            self.remote_round_trip_time_measurements
        }
    }

    impl From<&StreamStats> for StatsSnapshot {
        fn from(stream_stats: &StreamStats) -> Self {
            Self {
                rtp_stats: stream_stats.rtp_stats.clone(),
                rtcp_stats: stream_stats.rtcp_stats.clone(),
                remote_packets_sent: stream_stats.remote_packets_sent,
                remote_bytes_sent: stream_stats.remote_bytes_sent,
                remote_reports_sent: stream_stats.remote_reports_sent,
                remote_round_trip_time: stream_stats.remote_round_trip_time,
                remote_total_round_trip_time: stream_stats.remote_total_round_trip_time,
                remote_round_trip_time_measurements: stream_stats
                    .remote_round_trip_time_measurements,
            }
        }
    }
}

/// Types related to outbound RTP streams.
mod outbound {
    use std::time::SystemTime;

    use tokio::time::{Duration, Instant};

    use super::{RTCPStats, RTPStats};

    #[derive(Debug, Clone)]
    /// Stats collected for an outbound RTP stream.
    /// Contains both stats relating to the outbound stream and remote stats for the corresponding
    /// inbound stream.
    pub(super) struct StreamStats {
        /// Sent RTP stats.
        pub(super) rtp_stats: RTPStats,
        /// Common RTCP stats derived from inbound and outbound RTCP packets.
        pub(super) rtcp_stats: RTCPStats,

        /// The last time any stats where update, used for garbage collection to remove obsolete stats.
        last_update: Instant,

        /// The first value of extended seq num that was sent in an SR for this SSRC. [`None`] before
        /// the first SR is sent.
        ///
        /// Used to calculate packet statistic for remote stats.
        initial_outbound_ext_seq_num: Option<u32>,

        /// The number of inbound packets received by the remote side for this stream.
        remote_packets_received: u64,

        /// The number of lost packets reported by the remote for this tream.
        remote_total_lost: u32,

        /// The estimated remote jitter for this stream in timestamp units.
        remote_jitter: u32,

        /// The last remote round trip time measurement in ms. [`None`] if no round trip time has
        /// been derived yet, or if it wasn't possible to derive it.
        remote_round_trip_time: Option<f64>,

        /// The cumulative total round trip times reported in ms.
        remote_total_round_trip_time: f64,

        /// The total number of measurements of the remote round trip time.
        remote_round_trip_time_measurements: u64,

        /// The latest fraction lost value from RR.
        remote_fraction_lost: Option<u8>,
    }

    impl Default for StreamStats {
        fn default() -> Self {
            Self {
                rtp_stats: RTPStats::default(),
                rtcp_stats: RTCPStats::default(),
                last_update: Instant::now(),
                initial_outbound_ext_seq_num: None,
                remote_packets_received: 0,
                remote_total_lost: 0,
                remote_jitter: 0,
                remote_round_trip_time: None,
                remote_total_round_trip_time: 0.0,
                remote_round_trip_time_measurements: 0,
                remote_fraction_lost: None,
            }
        }
    }

    impl StreamStats {
        pub(super) fn snapshot(&self) -> StatsSnapshot {
            self.into()
        }

        pub(super) fn mark_updated(&mut self) {
            self.last_update = Instant::now();
        }

        pub(super) fn duration_since_last_update(&self) -> Duration {
            self.last_update.elapsed()
        }

        pub(super) fn update_remote_inbound_packets_received(
            &mut self,
            rr_ext_seq_num: u32,
            rr_total_lost: u32,
        ) {
            if let Some(initial_ext_seq_num) = self.initial_outbound_ext_seq_num {
                // Total number of RTP packets received for this SSRC.
                // At the receiving endpoint, this is calculated as defined in [RFC3550] section 6.4.1.
                // At the sending endpoint the packetsReceived is estimated by subtracting the
                // Cumulative Number of Packets Lost from the Extended Highest Sequence Number Received,
                // both reported in the RTCP Receiver Report, and then subtracting the
                // initial Extended Sequence Number that was sent to this SSRC in a RTCP Sender Report and then adding one,
                // to mirror what is discussed in Appendix A.3 in [RFC3550], but for the sender side.
                // If no RTCP Receiver Report has been received yet, then return 0.
                self.remote_packets_received =
                    (rr_ext_seq_num as u64) - (rr_total_lost as u64) - (initial_ext_seq_num as u64)
                        + 1;
            }
        }

        #[inline(always)]
        pub(super) fn record_sr_ext_seq_num(&mut self, seq_num: u32) {
            // Only record the initial value
            if self.initial_outbound_ext_seq_num.is_none() {
                self.initial_outbound_ext_seq_num = Some(seq_num);
            }
        }

        pub(super) fn record_remote_round_trip_time(&mut self, round_trip_time: Option<f64>) {
            // Store the latest measurement, even if it's None.
            self.remote_round_trip_time = round_trip_time;

            if let Some(rtt) = round_trip_time {
                // Only if we have a valid measurement do we update the totals
                self.remote_total_round_trip_time += rtt;
                self.remote_round_trip_time_measurements += 1;
            }
        }

        pub(super) fn update_remote_fraction_lost(&mut self, fraction_lost: u8) {
            self.remote_fraction_lost = Some(fraction_lost);
        }

        pub(super) fn update_remote_jitter(&mut self, jitter: u32) {
            self.remote_jitter = jitter;
        }

        pub(super) fn update_remote_total_lost(&mut self, lost: u32) {
            self.remote_total_lost = lost;
        }
    }

    /// A point in time snapshot of the stream stats for an outbound RTP stream.
    ///
    /// Created by [`StreamStats::snapshot`].
    #[derive(Debug)]
    pub struct StatsSnapshot {
        /// Sent RTP stats.
        rtp_stats: RTPStats,
        /// Common RTCP stats derived from inbound and outbound RTCP packets.
        rtcp_stats: RTCPStats,

        /// The number of inbound packets received by the remote side for this stream.
        remote_packets_received: u64,

        /// The number of lost packets reported by the remote for this tream.
        remote_total_lost: u32,

        /// The estimated remote jitter for this stream in timestamp units.
        remote_jitter: u32,

        /// The most recent remote round trip time in milliseconds.
        remote_round_trip_time: Option<f64>,

        /// The cumulative total round trip times reported in ms.
        remote_total_round_trip_time: f64,

        /// The total number of measurements of the remote round trip time.
        remote_round_trip_time_measurements: u64,

        /// The fraction of packets lost reported for this stream.
        /// Calculated as defined in [RFC3550](https://www.rfc-editor.org/rfc/rfc3550) section 6.4.1 and Appendix A.3.
        remote_fraction_lost: Option<f64>,
    }

    impl StatsSnapshot {
        pub fn packets_sent(&self) -> u64 {
            self.rtp_stats.packets
        }

        pub fn payload_bytes_sent(&self) -> u64 {
            self.rtp_stats.payload_bytes
        }

        pub fn header_bytes_sent(&self) -> u64 {
            self.rtp_stats.header_bytes
        }

        pub fn last_packet_sent_timestamp(&self) -> Option<SystemTime> {
            self.rtp_stats.last_packet_timestamp
        }

        pub fn nacks_received(&self) -> u64 {
            self.rtcp_stats.nack_count
        }

        pub fn firs_received(&self) -> u64 {
            self.rtcp_stats.fir_count
        }

        pub fn plis_received(&self) -> u64 {
            self.rtcp_stats.pli_count
        }

        /// Packets received on the remote side.
        pub fn remote_packets_received(&self) -> u64 {
            self.remote_packets_received
        }

        /// The number of lost packets reported by the remote for this tream.
        pub fn remote_total_lost(&self) -> u32 {
            self.remote_total_lost
        }

        /// The estimated remote jitter for this stream in timestamp units.
        pub fn remote_jitter(&self) -> u32 {
            self.remote_jitter
        }

        /// The latest RTT in ms if enough data is available to measure it.
        pub fn remote_round_trip_time(&self) -> Option<f64> {
            self.remote_round_trip_time
        }

        /// Total RTT in ms.
        pub fn remote_total_round_trip_time(&self) -> f64 {
            self.remote_total_round_trip_time
        }

        /// The number of RTT measurements so far.
        pub fn remote_round_trip_time_measurements(&self) -> u64 {
            self.remote_round_trip_time_measurements
        }

        /// The latest fraction lost value from the remote or None if it hasn't been reported yet.
        pub fn remote_fraction_lost(&self) -> Option<f64> {
            self.remote_fraction_lost
        }
    }

    impl From<&StreamStats> for StatsSnapshot {
        fn from(stream_stats: &StreamStats) -> Self {
            Self {
                rtp_stats: stream_stats.rtp_stats.clone(),
                rtcp_stats: stream_stats.rtcp_stats.clone(),
                remote_packets_received: stream_stats.remote_packets_received,
                remote_total_lost: stream_stats.remote_total_lost,
                remote_jitter: stream_stats.remote_jitter,
                remote_round_trip_time: stream_stats.remote_round_trip_time,
                remote_total_round_trip_time: stream_stats.remote_total_round_trip_time,
                remote_round_trip_time_measurements: stream_stats
                    .remote_round_trip_time_measurements,
                remote_fraction_lost: stream_stats
                    .remote_fraction_lost
                    .map(|fraction| (fraction as f64) / (u8::MAX as f64)),
            }
        }
    }
}

#[derive(Default, Debug)]
struct StatsContainer {
    inbound_stats: HashMap<u32, inbound::StreamStats>,
    outbound_stats: HashMap<u32, outbound::StreamStats>,
}

impl StatsContainer {
    fn get_or_create_inbound_stream_stats(&mut self, ssrc: u32) -> &mut inbound::StreamStats {
        self.inbound_stats.entry(ssrc).or_default()
    }

    fn get_or_create_outbound_stream_stats(&mut self, ssrc: u32) -> &mut outbound::StreamStats {
        self.outbound_stats.entry(ssrc).or_default()
    }

    fn get_inbound_stats(&self, ssrc: u32) -> Option<&inbound::StreamStats> {
        self.inbound_stats.get(&ssrc)
    }

    fn get_outbound_stats(&self, ssrc: u32) -> Option<&outbound::StreamStats> {
        self.outbound_stats.get(&ssrc)
    }

    fn remove_stale_entries(&mut self) {
        const MAX_AGE: Duration = Duration::from_secs(60);

        self.inbound_stats
            .retain(|_, s| s.duration_since_last_update() < MAX_AGE);
        self.outbound_stats
            .retain(|_, s| s.duration_since_last_update() < MAX_AGE);
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

    /// A wall clock timestamp for when the last packet was sent or received encoded as milliseconds since
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
    /// The number of FIRs sent or received
    fir_count: u64,

    /// The number of PLIs sent or received
    pli_count: u64,

    /// The number of NACKs sent or received
    nack_count: u64,
}

impl RTCPStats {
    #[allow(clippy::too_many_arguments)]
    fn update(&mut self, fir_count: Option<u64>, pli_count: Option<u64>, nack_count: Option<u64>) {
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
    fn test_rtcp_stats() {
        let mut stats: RTCPStats = Default::default();
        assert_eq!(
            (stats.fir_count(), stats.pli_count(), stats.nack_count()),
            (0, 0, 0),
        );

        stats.update(Some(1), Some(2), Some(3));

        assert_eq!(
            (stats.fir_count(), stats.pli_count(), stats.nack_count()),
            (1, 2, 3),
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
