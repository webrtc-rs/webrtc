use crate::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::peer_connection::certificate::RTCCertificate;

use ice::agent::agent_stats::{CandidatePairStats, CandidateStats};
use ice::candidate::{CandidatePairState, CandidateType};
use ice::network_type::NetworkType;
use stats_collector::StatsCollector;

use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Instant;

pub mod stats_collector;

pub enum SourceStatsType {
    CandidatePair(CandidatePairStats),
    LocalCandidate(CandidateStats),
    RemoteCandidate(CandidateStats),
}

pub enum StatsReportType {
    CandidatePair(ICECandidatePairStats),
    CertificateStats(CertificateStats),
    LocalCandidate(ICECandidateStats),
    RemoteCandidate(ICECandidateStats),
    SCTPTransport(ICETransportStats),
    Transport(ICETransportStats),
}

impl From<SourceStatsType> for StatsReportType {
    fn from(stats: SourceStatsType) -> Self {
        match stats {
            SourceStatsType::CandidatePair(stats) => StatsReportType::CandidatePair(stats.into()),
            SourceStatsType::LocalCandidate(stats) => StatsReportType::LocalCandidate(stats.into()),
            SourceStatsType::RemoteCandidate(stats) => {
                StatsReportType::RemoteCandidate(stats.into())
            }
        }
    }
}

pub struct StatsReport {}

impl From<Arc<Mutex<StatsCollector>>> for StatsReport {
    fn from(_collector: Arc<Mutex<StatsCollector>>) -> Self {
        StatsReport {}
    }
}

// TODO: should timestamps be cast here, or during serialization?
// func statsTimestampFrom(t time.Time) StatsTimestamp {
// 	return StatsTimestamp(t.UnixNano() / int64(time.Millisecond))
// }

pub struct ICECandidatePairStats {
    timestamp: Instant, // StatsTimestamp
    id: String,
    local_candidate_id: String,
    remote_candidate_id: String,
    state: CandidatePairState,
    nominated: bool,
    packets_sent: u32,
    packets_received: u32,
    bytes_sent: u64,
    bytes_received: u64,
    last_packet_sent_timestamp: Instant,    // statsTimestampFrom
    last_packet_received_timstamp: Instant, // statsTimestampFrom
    first_request_timestamp: Instant,       // statsTimestampFrom
    last_request_timestamp: Instant,        // statsTimestampFrom
    total_round_trip_time: f64,
    current_round_trip_time: f64,
    available_outgoing_bitrate: f64,
    available_incoming_bitrate: f64,
    circuit_breaker_trigger_count: u32,
    requests_received: u64,
    requests_sent: u64,
    responses_received: u64,
    responses_sent: u64,
    retransmissions_sent: u64,
    consent_requests_sent: u64,
    consent_expired_timestamp: Instant, // statsTimestampFrom
}

impl From<CandidatePairStats> for ICECandidatePairStats {
    fn from(stats: CandidatePairStats) -> Self {
        ICECandidatePairStats {
            timestamp: stats.timestamp,
            id: format!("{}-{}", stats.local_candidate_id, stats.remote_candidate_id),
            local_candidate_id: stats.local_candidate_id,
            remote_candidate_id: stats.remote_candidate_id,
            state: stats.state,
            nominated: stats.nominated,
            packets_sent: stats.packets_sent,
            packets_received: stats.packets_received,
            bytes_sent: stats.bytes_sent,
            bytes_received: stats.bytes_received,
            last_packet_sent_timestamp: stats.last_packet_sent_timestamp,
            last_packet_received_timstamp: stats.last_packet_received_timestamp,
            first_request_timestamp: stats.first_request_timestamp,
            last_request_timestamp: stats.last_request_timestamp,
            total_round_trip_time: stats.total_round_trip_time,
            current_round_trip_time: stats.current_round_trip_time,
            available_outgoing_bitrate: stats.available_outgoing_bitrate,
            available_incoming_bitrate: stats.available_incoming_bitrate,
            circuit_breaker_trigger_count: stats.circuit_breaker_trigger_count,
            requests_received: stats.requests_received,
            requests_sent: stats.requests_sent,
            responses_received: stats.responses_received,
            responses_sent: stats.responses_sent,
            retransmissions_sent: stats.retransmissions_sent,
            consent_requests_sent: stats.consent_requests_sent,
            consent_expired_timestamp: stats.consent_expired_timestamp,
        }
    }
}

pub struct ICECandidateStats {
    timestamp: Instant,
    id: String,
    candidate_type: CandidateType,
    deleted: bool,
    ip: String,
    network_type: NetworkType,
    port: u16,
    priority: u32,
    relay_protocol: String,
    url: String,
}

impl From<CandidateStats> for ICECandidateStats {
    fn from(stats: CandidateStats) -> Self {
        ICECandidateStats {
            timestamp: stats.timestamp,
            id: stats.id,
            network_type: stats.network_type,
            ip: stats.ip,
            port: stats.port,
            candidate_type: stats.candidate_type,
            priority: stats.priority,
            url: stats.url,
            relay_protocol: stats.relay_protocol,
            deleted: stats.deleted,
        }
    }
}

pub struct ICETransportStats {
    timestamp: Instant,
    id: String,
    // bytes_received: u64,
    // bytes_sent: u64,
}

impl ICETransportStats {
    pub(crate) fn new(id: String) -> Self {
        ICETransportStats {
            id,
            timestamp: Instant::now(),
        }
    }
}

pub struct CertificateStats {
    timestamp: Instant,
    id: String,
    // base64_certificate: String,
    fingerprint: String,
    fingerprint_algorithm: String,
    // issuer_certificate_id: String,
}

impl CertificateStats {
    pub(crate) fn new(cert: &RTCCertificate, fingerprint: RTCDtlsFingerprint) -> Self {
        CertificateStats {
            timestamp: Instant::now(),
            id: cert.stats_id.clone(),
            // TODO: base64_certificate
            fingerprint: fingerprint.value,
            fingerprint_algorithm: fingerprint.algorithm,
            // TODO: issuer_certificate_id
        }
    }
}
